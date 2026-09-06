use crate::provider::EventSink;
use crate::types::CoachEvent;
use optcg_core::GameState;
use optcg_database::CardRepository;
use optcg_rules::{CombatMath, RulesEngine};

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

/// Facts gathered from the live match, ready to prepend to a chat turn.
#[derive(Debug, Clone, Default)]
pub struct GroundedContext {
    pub sections: Vec<(String, String)>,
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
    sink: &EventSink,
) -> GroundedContext {
    let mut context = GroundedContext::default();

    sink(CoachEvent::status("Reading board state"));
    let board = board_readout(state);
    sink(CoachEvent::tool("board_readout", board_summary(state)));
    context.push("Board", board);

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

    context.push("Decks", deck_readout(decks));
    context
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
            if analysis.recommended_block { "" } else { "not " }
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
        lines.push(
            "Your exact list is not saved, so treat deck contents as unknown.".to_string(),
        );
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

        let context = build_context(&sample_state(), &repo, &decks, &sink);
        let prompt = context.to_prompt();

        assert!(prompt.contains("## Board"), "missing board section: {prompt}");
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

        build_context(&sample_state(), &repo, &DeckContext::default(), &sink);
        let events = recorder.events();

        assert!(
            events.iter().any(|e| matches!(e, CoachEvent::Status(s) if s.contains("board"))),
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

        let context = build_context(&sample_state(), &repo, &DeckContext::default(), &sink);
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

        let peaceful = build_context(&sample_state(), &repo, &DeckContext::default(), &sink);
        assert!(!peaceful.to_prompt().contains("## Combat math"));

        let mut fighting = sample_state();
        fighting.combat.active = true;
        fighting.combat.attacker_id = Some("ST01-002".into());
        fighting.combat.target_is_leader = true;
        let context = build_context(&fighting, &repo, &DeckContext::default(), &sink);
        assert!(context.to_prompt().contains("Combat: ST01-002 attacking leader"));
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
