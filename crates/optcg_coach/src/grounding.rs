use crate::provider::EventSink;
use crate::types::{CoachEvent, StateFingerprint};
use optcg_core::{GameState, PlayerState};
use optcg_database::CardRepository;
use optcg_rules::{CombatMath, RulesEngine};
use serde::{Deserialize, Serialize};

/// Deck context the desktop app already computes, passed in so this crate does
/// not need to know about the app's DTOs.
#[derive(Debug, Clone, Default)]
pub struct DeckContext {
    pub your_deck: String,
    pub your_leader: String,
    /// Lines like `4x Usopp (ST01-002)`, present only for an exact saved list.
    pub your_list: Vec<String>,
    pub opponent_deck: String,
    pub opponent_leader: String,
    pub plan: Option<String>,
    pub vs_opponent: Option<String>,
}

/// Which parts of the live context a turn may send.
///
/// Everything is shared by default, since board-aware coaching is the point of
/// the app. But a rules or matchup question does not need your deck list
/// shipped to a third-party API, and there was no way to withhold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextScope {
    /// The live position and everything read off it: board, counter estimate,
    /// phase guidance, combat math, ranked options.
    pub board: bool,
    /// Your saved deck list, leader, and matchup plan.
    pub deck: bool,
}

impl Default for ContextScope {
    fn default() -> Self {
        Self {
            board: true,
            deck: true,
        }
    }
}

impl ContextScope {
    /// Share nothing: answers come from rules knowledge alone.
    pub const NOTHING: Self = Self {
        board: false,
        deck: false,
    };

    pub fn shares_anything(self) -> bool {
        self.board || self.deck
    }

    /// What this scope keeps back, for the model and the UI to name.
    pub fn withheld(self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if !self.board {
            out.push("the live board");
        }
        if !self.deck {
            out.push("your deck list");
        }
        out
    }
}

/// Facts gathered from the live match, ready to prepend to a chat turn.
#[derive(Debug, Clone, Default)]
pub struct GroundedContext {
    pub sections: Vec<(String, String)>,
    /// The position these facts were read from, absent when the board was not
    /// shared. Answers with no position cannot go stale, so leaving this unset
    /// is what keeps board changes from interrupting them.
    pub fingerprint: Option<StateFingerprint>,
}

impl GroundedContext {
    /// Render as the briefing block handed to the model.
    pub fn to_prompt(&self) -> String {
        self.sections
            .iter()
            .map(|(heading, body)| format!("## {heading}\n{body}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn push(&mut self, heading: &str, body: impl Into<String>) {
        let body = body.into();
        if !body.trim().is_empty() {
            self.sections.push((heading.to_string(), body));
        }
    }
}

/// Upper bound on the counter power an opponent could add to a fight.
///
/// The opponent's hand is hidden, so this is deliberately not a claim about
/// what they hold. It combines two things we do observe: the counter values on
/// cards they have already revealed this match, and how many cards are in
/// their hand.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CounterEstimate {
    pub hand_size: u32,
    /// Distinct non-zero counter values seen on their revealed cards, ascending.
    pub observed_values: Vec<i32>,
    /// Largest single counter observed, i.e. the most one card could add.
    pub max_single: i32,
    /// `max_single * hand_size`: the ceiling if every card held were their
    /// biggest counter. A bound to plan against, not a prediction.
    pub worst_case_total: i32,
}

impl CounterEstimate {
    fn summary(&self) -> String {
        if self.max_single == 0 {
            return format!("no counters revealed yet, {} cards in hand", self.hand_size);
        }
        format!(
            "≤{} from {} cards (max single {})",
            self.worst_case_total, self.hand_size, self.max_single
        )
    }

    fn readout(&self) -> String {
        if self.hand_size == 0 {
            return "Opponent has no cards in hand, so they cannot counter.".to_string();
        }
        if self.max_single == 0 {
            return format!(
                "Opponent holds {} cards. No counter values have been revealed this \
                 match, so treat their counter potential as unknown.",
                self.hand_size
            );
        }
        format!(
            "Opponent holds {} cards. Cards they have revealed carry counters of {}. \
             If every card in hand were their largest counter ({}), they could add at \
             most {}. This is an upper bound from revealed cards, not their actual hand.",
            self.hand_size,
            self.observed_values
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            self.max_single,
            self.worst_case_total
        )
    }
}

/// Estimate the opponent's counter ceiling from cards they have revealed.
pub fn estimate_counters(opponent: &PlayerState, repo: &CardRepository<'_>) -> CounterEstimate {
    let mut values: Vec<i32> = Vec::new();

    // One lookup per iteration: each `get_by_id` takes the database lock, so
    // they must not be combined into a single expression.
    for card_id in &opponent.known_cards {
        let Ok(card) = repo.get_by_id(card_id) else {
            continue;
        };
        if card.counter > 0 {
            values.push(card.counter);
        }
    }

    values.sort_unstable();
    values.dedup();
    let max_single = values.last().copied().unwrap_or(0);

    CounterEstimate {
        hand_size: opponent.hand_count,
        observed_values: values,
        max_single,
        worst_case_total: max_single * opponent.hand_count as i32,
    }
}

/// The HUD is always the player at index 0.
const YOU: u8 = 0;

/// True when the player has a choice worth coaching right now.
///
/// Used to keep automatic reads quiet through the opponent's turn and the
/// phases that play themselves, where unprompted advice is just noise.
pub fn is_decision_point(state: &GameState) -> bool {
    // Their attack is resolving against you: block and counter decisions are
    // the highest-value moment to be coached, on either player's turn.
    if state.combat.active && state.combat.target_player == Some(YOU) {
        return true;
    }
    // Your own turn, in the phases where you choose what to do.
    if state.active_player == YOU {
        return matches!(
            state.phase,
            optcg_core::Phase::Main | optcg_core::Phase::Combat
        );
    }
    false
}

/// Identify the board position an answer is grounded on.
///
/// Only fields that would make advice wrong are included, so the turn is not
/// interrupted by unrelated event traffic.
pub fn fingerprint(state: &GameState) -> StateFingerprint {
    let you = state.player_one();
    let opp = state.player_two();

    let combat = if state.combat.active {
        format!(
            "combat:{}>{}{}",
            state.combat.attacker_id.as_deref().unwrap_or("?"),
            if state.combat.target_is_leader {
                "leader"
            } else {
                state.combat.target_id.as_deref().unwrap_or("?")
            },
            if state.combat.blocker_offered {
                "+b"
            } else {
                ""
            }
        )
    } else {
        "combat:none".to_string()
    };

    let digest = format!(
        "t{} p{:?} a{} | {} | {} | {}",
        state.turn_number,
        state.phase,
        state.active_player,
        side_digest(you),
        side_digest(opp),
        combat
    );

    StateFingerprint {
        label: format!(
            "turn {} · {:?} · life {}-{}",
            state.turn_number, state.phase, you.life, opp.life
        ),
        digest,
    }
}

fn side_digest(player: &PlayerState) -> String {
    let mut board: Vec<String> = player
        .characters
        .iter()
        .map(|card| {
            format!(
                "{}{}{}",
                card.card_id,
                if card.rested { "r" } else { "" },
                if card.attached_don > 0 {
                    format!("+{}", card.attached_don)
                } else {
                    String::new()
                }
            )
        })
        .collect();
    // Board order is an artifact of observation, not part of the position.
    board.sort_unstable();

    format!(
        "L{} h{} d{}/{} ldr{}+{} [{}]",
        player.life,
        player.hand_count,
        player.don_active,
        player.don_rested,
        player.leader.card_id,
        player.leader.attached_don,
        board.join(",")
    )
}

/// The instruction block that defines the coach's job and its limits.
pub const SYSTEM_PROMPT: &str = "\
You are the in-game coach inside the OPTCG Companion HUD, helping the player \
during a live One Piece Card Game match.

Ground every answer in the MATCH BRIEFING below. It is observed from the \
player's simulator and is the only reliable source of board state. If the \
briefing does not contain what you need, say what is missing instead of \
inventing a board, a card, or an opponent's hand.

Answer like a coach mid-match: lead with the recommendation, then give the \
short reason. Prefer concrete lines ('attack the leader with Zoro, hold Usopp \
to block') over general theory. Be brief; the HUD is a narrow overlay. Never \
claim to know hidden information such as the opponent's hand.";

/// Run the read-only analysis tools and assemble the match briefing.
///
/// Each tool reports through `sink` so the HUD can show what the agent looked
/// at while the answer streams. Nothing here mutates game state: the coach can
/// read the board and the rules engine, and nothing else.
pub fn build_context(
    state: &GameState,
    repo: &CardRepository<'_>,
    decks: &DeckContext,
    scope: ContextScope,
    sink: &EventSink,
) -> GroundedContext {
    let mut context = GroundedContext::default();

    if scope.board {
        sink(CoachEvent::status("Reading board state"));
        let board = board_readout(state);
        sink(CoachEvent::tool("board_readout", board_summary(state)));
        context.push("Board", board);

        if let Some(actions) = recent_actions(state) {
            sink(CoachEvent::tool(
                "recent_actions",
                format!("{} recent", actions.lines().count()),
            ));
            context.push("Recent actions", actions);
        }

        sink(CoachEvent::status("Estimating opponent counters"));
        let counters = estimate_counters(state.player_two(), repo);
        sink(CoachEvent::tool("counter_estimate", counters.summary()));
        context.push("Opponent counter range", counters.readout());

        sink(CoachEvent::status("Checking the current phase"));
        let phase_coach = RulesEngine::phase_coach(state);
        context.push("Phase guidance", phase_coach);

        if state.combat.active {
            sink(CoachEvent::status("Running combat math"));
            if let Some(analysis) = CombatMath::analyze_current_combat(state, repo) {
                sink(CoachEvent::tool(
                    "combat_math",
                    format!(
                        "{:?} · needs {} counter",
                        analysis.survival_status, analysis.required_counter
                    ),
                ));
                context.push("Combat math", combat_readout(&analysis));
            }
        }

        sink(CoachEvent::status("Ranking legal actions"));
        match RulesEngine::rank_actions(state, repo) {
            Ok(options) if !options.is_empty() => {
                let shown = options.len().min(6);
                sink(CoachEvent::tool(
                    "rank_actions",
                    format!("{shown} of {} options", options.len()),
                ));
                let body = options
                    .iter()
                    .take(shown)
                    .enumerate()
                    .map(|(i, option)| {
                        format!(
                            "{}. {} (score {:.2}) — {}",
                            i + 1,
                            option.action.description,
                            option.score,
                            option.reasoning
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                context.push("Ranked options", body);
            }
            Ok(_) => sink(CoachEvent::tool("rank_actions", "no legal actions")),
            Err(e) => {
                tracing::debug!(error = %e, "could not rank actions for coach context");
                sink(CoachEvent::tool("rank_actions", "unavailable"));
            }
        }
    }

    if scope.deck {
        context.push("Decks", deck_readout(decks));
    }

    // Name what was held back, or the model fills the gap by inventing a board.
    let withheld = scope.withheld();
    if !withheld.is_empty() {
        context.push(
            "Withheld",
            format!(
                "The player chose not to share {}. Do not guess at what you \
                 cannot see — answer from rules knowledge and say what you \
                 would need to look at.",
                withheld.join(" or ")
            ),
        );
    }

    // Emitted last, once the position that was actually read is known, so the
    // UI can label the answer and detect it going stale. An answer given
    // without the board has no position and so can never go stale.
    if scope.board {
        let position = fingerprint(state);
        sink(CoachEvent::StateSync(position.clone()));
        context.fingerprint = Some(position);
    }
    context
}

/// How many of the most recent actions the briefing carries.
///
/// Enough to show the shape of the current turn without spending the prompt on
/// history the model cannot act on.
const RECENT_ACTIONS: usize = 12;

/// The last few actions, oldest first, or `None` before anything has happened.
///
/// This is the one thing the position readout cannot convey: whether life went
/// from 4 to 2 this turn or ten turns ago, and what the opponent has been
/// doing to get here.
fn recent_actions(state: &GameState) -> Option<String> {
    let start = state.event_log.len().saturating_sub(RECENT_ACTIONS);
    let recent = state.event_log.get(start..)?;
    if recent.is_empty() {
        return None;
    }
    Some(recent.join("\n"))
}

fn board_summary(state: &GameState) -> String {
    format!(
        "turn {} · {:?} · life {}-{}",
        state.turn_number,
        state.phase,
        state.player_one().life,
        state.player_two().life
    )
}

fn board_readout(state: &GameState) -> String {
    let you = state.player_one();
    let opp = state.player_two();
    let mut lines = vec![
        format!(
            "Turn {}, {:?} phase, active player: {}",
            state.turn_number,
            state.phase,
            if state.active_player == 0 {
                "you"
            } else {
                "opponent"
            }
        ),
        format!("You: {}", side_readout(you)),
        format!("Opponent: {}", side_readout(opp)),
    ];

    if state.combat.active {
        let attacker = state.combat.attacker_id.as_deref().unwrap_or("?");
        let target = if state.combat.target_is_leader {
            "leader".to_string()
        } else {
            state
                .combat
                .target_id
                .clone()
                .unwrap_or_else(|| "?".to_string())
        };
        lines.push(format!(
            "Combat: {attacker} attacking {target}{}",
            if state.combat.blocker_offered {
                ", blocker offered"
            } else {
                ""
            }
        ));
    }
    lines.join("\n")
}

fn side_readout(player: &optcg_core::PlayerState) -> String {
    let board = if player.characters.is_empty() {
        "empty board".to_string()
    } else {
        player
            .characters
            .iter()
            .map(|card| {
                let mut label = card.card_id.clone();
                if card.rested {
                    label.push_str(" (rested)");
                }
                if card.attached_don > 0 {
                    label.push_str(&format!(" +{} DON", card.attached_don));
                }
                label
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    format!(
        "life {}, hand {}, deck {}, DON {} active / {} rested, leader {} ({} power); board: {}",
        player.life,
        player.hand_count,
        player.deck_count,
        player.don_active,
        player.don_rested,
        if player.leader.card_id.is_empty() {
            "unknown"
        } else {
            player.leader.card_id.as_str()
        },
        player.leader.power,
        board
    )
}

fn combat_readout(analysis: &optcg_rules::CombatAnalysis) -> String {
    let mut lines = vec![
        format!(
            "Attacker {} vs defender {} (differential {})",
            analysis.attacker_power, analysis.defender_power, analysis.power_differential
        ),
        format!("Status: {:?}", analysis.survival_status),
        format!(
            "Counter needed to survive: {}",
            analysis.required_counter.max(0)
        ),
    ];
    if analysis.lethal_to_leader {
        lines.push("This attack is lethal to the leader if it connects.".into());
    }
    if analysis.blocker_available {
        lines.push(format!(
            "A blocker is available; blocking is {}recommended.",
            if analysis.recommended_block {
                ""
            } else {
                "not "
            }
        ));
    }
    lines.join("\n")
}

fn deck_readout(decks: &DeckContext) -> String {
    let mut lines = Vec::new();
    if !decks.your_deck.is_empty() {
        lines.push(format!(
            "Your deck: {} (leader {})",
            decks.your_deck, decks.your_leader
        ));
    }
    if !decks.opponent_deck.is_empty() {
        lines.push(format!(
            "Opponent deck: {} (leader {})",
            decks.opponent_deck, decks.opponent_leader
        ));
    }
    if let Some(plan) = decks.plan.as_ref().filter(|p| !p.trim().is_empty()) {
        lines.push(format!("Your game plan: {plan}"));
    }
    if let Some(vs) = decks.vs_opponent.as_ref().filter(|p| !p.trim().is_empty()) {
        lines.push(format!("Against this deck: {vs}"));
    }
    if !decks.your_list.is_empty() {
        lines.push(format!(
            "Your exact list ({} entries): {}",
            decks.your_list.len(),
            decks.your_list.join(", ")
        ));
    } else {
        lines.push("Your exact list is not saved, so treat deck contents as unknown.".to_string());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::test_support::recording_sink;
    use optcg_database::{AssetParser, Database};

    fn db() -> Database {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        db
    }

    fn sample_state() -> GameState {
        let mut state = GameState::new();
        state.turn_number = 4;
        state.player_one_mut().life = 3;
        state.player_one_mut().leader = optcg_core::LeaderState::new("ST01-001");
        state.player_two_mut().life = 2;
        state.player_two_mut().leader = optcg_core::LeaderState::new("ST01-001");
        state
    }

    #[test]
    fn context_includes_board_phase_and_decks() {
        let db = db();
        let repo = CardRepository::new(&db);
        let (sink, _recorder) = recording_sink();
        let decks = DeckContext {
            your_deck: "Red Luffy Aggro".into(),
            your_leader: "ST01-001".into(),
            your_list: vec!["4x Usopp (ST01-002)".into()],
            opponent_deck: "Green Zoro".into(),
            opponent_leader: "ST01-001".into(),
            plan: Some("Race with cheap attackers".into()),
            vs_opponent: None,
        };

        let context = build_context(
            &sample_state(),
            &repo,
            &decks,
            ContextScope::default(),
            &sink,
        );
        let prompt = context.to_prompt();

        assert!(
            prompt.contains("## Board"),
            "missing board section: {prompt}"
        );
        assert!(prompt.contains("life 3"), "own life missing: {prompt}");
        assert!(prompt.contains("Red Luffy Aggro"));
        assert!(prompt.contains("4x Usopp (ST01-002)"));
        assert!(prompt.contains("Race with cheap attackers"));
    }

    #[test]
    fn tools_report_progress_through_the_sink() {
        let db = db();
        let repo = CardRepository::new(&db);
        let (sink, recorder) = recording_sink();

        build_context(
            &sample_state(),
            &repo,
            &DeckContext::default(),
            ContextScope::default(),
            &sink,
        );
        let events = recorder.events();

        assert!(
            events
                .iter()
                .any(|e| matches!(e, CoachEvent::Status(s) if s.contains("board"))),
            "expected a board status event: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CoachEvent::ToolRun(t) if t.tool == "board_readout")),
            "expected a board_readout tool event: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CoachEvent::ToolRun(t) if t.tool == "rank_actions")),
            "expected a rank_actions tool event: {events:?}"
        );
        assert!(
            !events.iter().any(CoachEvent::is_terminal),
            "grounding must not emit the terminal frame"
        );
    }

    #[test]
    fn missing_deck_list_is_stated_rather_than_implied() {
        let db = db();
        let repo = CardRepository::new(&db);
        let (sink, _recorder) = recording_sink();

        let context = build_context(
            &sample_state(),
            &repo,
            &DeckContext::default(),
            ContextScope::default(),
            &sink,
        );
        assert!(
            context.to_prompt().contains("not saved"),
            "the model should be told the list is unknown"
        );
    }

    #[test]
    fn combat_section_appears_only_during_combat() {
        let db = db();
        let repo = CardRepository::new(&db);
        let (sink, _recorder) = recording_sink();

        let peaceful = build_context(
            &sample_state(),
            &repo,
            &DeckContext::default(),
            ContextScope::default(),
            &sink,
        );
        assert!(!peaceful.to_prompt().contains("## Combat math"));

        let mut fighting = sample_state();
        fighting.combat.active = true;
        fighting.combat.attacker_id = Some("ST01-002".into());
        fighting.combat.target_is_leader = true;
        let context = build_context(
            &fighting,
            &repo,
            &DeckContext::default(),
            ContextScope::default(),
            &sink,
        );
        assert!(context
            .to_prompt()
            .contains("Combat: ST01-002 attacking leader"));
    }

    #[test]
    fn counter_estimate_bounds_from_revealed_cards() {
        let db = db();
        let repo = CardRepository::new(&db);
        let mut state = sample_state();
        {
            let opp = state.player_two_mut();
            opp.hand_count = 4;
            opp.known_cards = vec!["ST01-002".into(), "ST01-003".into()];
        }

        let estimate = estimate_counters(state.player_two(), &repo);

        assert_eq!(estimate.hand_size, 4);
        assert!(
            !estimate.observed_values.is_empty(),
            "seeded cards should carry counters"
        );
        assert!(
            estimate.observed_values.windows(2).all(|w| w[0] < w[1]),
            "values should be sorted and deduplicated: {:?}",
            estimate.observed_values
        );
        assert_eq!(
            estimate.max_single,
            *estimate.observed_values.last().unwrap()
        );
        assert_eq!(
            estimate.worst_case_total,
            estimate.max_single * 4,
            "worst case is the largest counter across every card held"
        );
    }

    #[test]
    fn counter_estimate_states_what_it_does_not_know() {
        let db = db();
        let repo = CardRepository::new(&db);
        let mut state = sample_state();
        state.player_two_mut().hand_count = 5;

        // Nothing revealed yet.
        let estimate = estimate_counters(state.player_two(), &repo);
        assert_eq!(estimate.max_single, 0);
        assert_eq!(estimate.worst_case_total, 0);
        assert!(
            estimate.readout().contains("unknown"),
            "an empty estimate must not imply they cannot counter: {}",
            estimate.readout()
        );

        // An empty hand is a real certainty, unlike an empty observation set.
        state.player_two_mut().hand_count = 0;
        let empty_hand = estimate_counters(state.player_two(), &repo);
        assert!(empty_hand.readout().contains("cannot counter"));
    }

    #[test]
    fn counter_estimate_ignores_unknown_card_ids() {
        let db = db();
        let repo = CardRepository::new(&db);
        let mut state = sample_state();
        state.player_two_mut().hand_count = 1;
        state.player_two_mut().known_cards = vec!["NOT-A-REAL-CARD".into()];

        let estimate = estimate_counters(state.player_two(), &repo);
        assert!(
            estimate.observed_values.is_empty(),
            "a card missing from the database should be skipped, not fatal"
        );
    }

    #[test]
    fn context_reports_the_position_it_read() {
        let db = db();
        let repo = CardRepository::new(&db);
        let (sink, recorder) = recording_sink();

        let context = build_context(
            &sample_state(),
            &repo,
            &DeckContext::default(),
            ContextScope::default(),
            &sink,
        );

        let position = context
            .fingerprint
            .clone()
            .expect("sharing the board should record a position");
        assert_eq!(position, fingerprint(&sample_state()));
        assert!(position.label.contains("turn 4"));

        let sync = recorder
            .events()
            .into_iter()
            .find_map(|e| match e {
                CoachEvent::StateSync(f) => Some(f),
                _ => None,
            })
            .expect("a state_sync frame should be emitted");
        assert_eq!(sync, position);
    }

    #[test]
    fn fingerprint_changes_only_for_material_moves() {
        let base = sample_state();
        let before = fingerprint(&base);

        // Noise that does not change the position.
        let mut noisy = base.clone();
        noisy.event_sequence += 25;
        noisy.player_one_mut().trash_count += 3;
        noisy.push_log("some log line");
        assert_eq!(
            fingerprint(&noisy).digest,
            before.digest,
            "event churn must not count as the board moving"
        );

        // Real changes.
        let mut damaged = base.clone();
        damaged.player_one_mut().life -= 1;
        assert_ne!(fingerprint(&damaged).digest, before.digest, "life matters");

        let mut donned = base.clone();
        donned.player_one_mut().don_active += 1;
        assert_ne!(fingerprint(&donned).digest, before.digest, "DON matters");

        let mut drawn = base.clone();
        drawn.player_two_mut().hand_count += 1;
        assert_ne!(
            fingerprint(&drawn).digest,
            before.digest,
            "opponent hand size matters"
        );

        let mut fighting = base.clone();
        fighting.combat.active = true;
        fighting.combat.attacker_id = Some("ST01-002".into());
        assert_ne!(
            fingerprint(&fighting).digest,
            before.digest,
            "combat starting matters"
        );
    }

    #[test]
    fn fingerprint_ignores_board_ordering() {
        let mut left = sample_state();
        let mut right = sample_state();
        let a = optcg_core::CardInstance::new("ST01-002", 0, optcg_core::Zone::Character);
        let b = optcg_core::CardInstance::new("ST01-003", 0, optcg_core::Zone::Character);

        left.player_one_mut().characters = vec![a.clone(), b.clone()];
        right.player_one_mut().characters = vec![b, a];

        assert_eq!(
            fingerprint(&left).digest,
            fingerprint(&right).digest,
            "observation order is not part of the position"
        );
    }

    fn context_with(scope: ContextScope) -> (GroundedContext, Vec<CoachEvent>) {
        let db = db();
        let repo = CardRepository::new(&db);
        let (sink, recorder) = recording_sink();
        let decks = DeckContext {
            your_deck: "Red Zoro".into(),
            your_leader: "OP01-001".into(),
            ..Default::default()
        };
        let context = build_context(&sample_state(), &repo, &decks, scope, &sink);
        (context, recorder.events())
    }

    #[test]
    fn the_briefing_carries_what_just_happened() {
        let db = db();
        let repo = CardRepository::new(&db);
        let (sink, recorder) = recording_sink();

        let mut state = sample_state();
        state.push_log("#1 PHASE_CHANGED Main");
        state.push_log("#2 ATTACK_DECLARED OP01-001 power=5000");

        let context = build_context(
            &state,
            &repo,
            &DeckContext::default(),
            ContextScope::default(),
            &sink,
        );
        let prompt = context.to_prompt();

        assert!(prompt.contains("## Recent actions"));
        assert!(prompt.contains("ATTACK_DECLARED OP01-001"));
        assert!(
            recorder.events().iter().any(|e| matches!(
                e,
                CoachEvent::ToolRun(run) if run.tool == "recent_actions"
            )),
            "the HUD should show that the log was read"
        );
    }

    #[test]
    fn an_empty_log_adds_no_section() {
        let db = db();
        let repo = CardRepository::new(&db);
        let (sink, _) = recording_sink();

        let context = build_context(
            &sample_state(),
            &repo,
            &DeckContext::default(),
            ContextScope::default(),
            &sink,
        );

        assert!(
            !context.to_prompt().contains("Recent actions"),
            "an empty heading would just waste prompt space"
        );
    }

    #[test]
    fn only_the_last_few_actions_are_carried() {
        let mut state = sample_state();
        for i in 0..60 {
            state.push_log(format!("#{i} PHASE_CHANGED Main"));
        }

        let actions = recent_actions(&state).expect("a populated log");
        assert_eq!(actions.lines().count(), RECENT_ACTIONS);
        assert!(
            actions.starts_with("#48 "),
            "the newest actions are the ones that matter: {actions}"
        );
        assert!(actions.ends_with("#59 PHASE_CHANGED Main"));
    }

    #[test]
    fn withholding_the_board_removes_it_and_everything_read_off_it() {
        let (context, events) = context_with(ContextScope {
            board: false,
            deck: true,
        });
        let prompt = context.to_prompt();

        for absent in [
            "## Board",
            "## Opponent counter range",
            "## Phase guidance",
            "## Ranked options",
        ] {
            assert!(!prompt.contains(absent), "{absent} should be withheld");
        }
        assert!(prompt.contains("Red Zoro"), "the deck was still shared");

        // The model has to be told, or it fills the gap by inventing a board.
        assert!(prompt.contains("## Withheld"));
        assert!(prompt.contains("the live board"));

        assert!(
            !events
                .iter()
                .any(|e| matches!(e, CoachEvent::ToolRun(_) | CoachEvent::StateSync(_))),
            "no board tool should run: {events:?}"
        );
        assert!(
            context.fingerprint.is_none(),
            "an answer without a position cannot go stale, so none is recorded"
        );
    }

    #[test]
    fn withholding_the_deck_keeps_the_board() {
        let (context, _) = context_with(ContextScope {
            board: true,
            deck: false,
        });
        let prompt = context.to_prompt();

        assert!(prompt.contains("## Board"));
        assert!(!prompt.contains("Red Zoro"), "the deck list was withheld");
        assert!(prompt.contains("your deck list"));
        assert!(
            context.fingerprint.is_some(),
            "a board-grounded answer still records its position"
        );
    }

    #[test]
    fn sharing_nothing_still_produces_a_usable_prompt() {
        let (context, events) = context_with(ContextScope::NOTHING);
        let prompt = context.to_prompt();

        assert!(!ContextScope::NOTHING.shares_anything());
        assert!(
            prompt.contains("the live board") && prompt.contains("your deck list"),
            "both omissions must be named: {prompt}"
        );
        assert!(
            prompt.contains("Do not guess"),
            "the model needs telling not to invent context"
        );
        assert!(
            !events.iter().any(|e| matches!(e, CoachEvent::ToolRun(_))),
            "nothing should be read at all"
        );
    }

    #[test]
    fn sharing_everything_names_no_omissions() {
        let (context, _) = context_with(ContextScope::default());
        assert!(ContextScope::default().shares_anything());
        assert!(ContextScope::default().withheld().is_empty());
        assert!(!context.to_prompt().contains("## Withheld"));
    }

    #[test]
    fn decision_points_are_where_the_player_actually_chooses() {
        use optcg_core::Phase;

        let mut state = sample_state();

        state.active_player = 0;
        for phase in [Phase::Main, Phase::Combat] {
            state.phase = phase;
            assert!(
                is_decision_point(&state),
                "{phase:?} on your turn is a decision point"
            );
        }
        for phase in [Phase::Draw, Phase::Don, Phase::End] {
            state.phase = phase;
            assert!(
                !is_decision_point(&state),
                "{phase:?} plays itself, so advice would be noise"
            );
        }

        // The opponent's turn is quiet.
        state.active_player = 1;
        state.phase = Phase::Main;
        assert!(!is_decision_point(&state));

        // Except when their attack is resolving against you, which is the
        // highest-value moment to be coached.
        state.combat.active = true;
        state.combat.target_player = Some(0);
        assert!(is_decision_point(&state));

        // Their attack on their own board is not your decision.
        state.combat.target_player = Some(1);
        assert!(!is_decision_point(&state));
    }

    #[test]
    fn empty_sections_are_dropped() {
        let mut context = GroundedContext::default();
        context.push("Kept", "body");
        context.push("Dropped", "   ");
        assert_eq!(context.sections.len(), 1);
        assert_eq!(context.to_prompt(), "## Kept\nbody");
    }
}
