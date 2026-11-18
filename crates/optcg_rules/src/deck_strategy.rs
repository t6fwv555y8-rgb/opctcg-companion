//! The "Game plan" shown in the HUD, built from data.
//!
//! Every line here is a fact with a source: how winning tournament lists for a
//! Leader are built, what the opponent's winning lists tend to run, what is in
//! your hand and on the board right now, and how your list compares. Nothing is
//! inferred from a deck's colour or name, and nothing is a hard-coded opinion
//! about a particular card. With too little data a section says so rather than
//! guessing.

use crate::deck_list::DeckListEntry;
use crate::leader_stats::{card_tags, CoreCard, LeaderStats};
use crate::meta::DeckComparison;
use optcg_core::{CardType, GameState, Phase};
use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};

/// Snapshot of one side's deck identity used for matchup advice.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeckProfile {
    pub name: String,
    pub leader_id: String,
    pub leader_name: String,
    pub leader_color: String,
    pub known_card_ids: Vec<String>,
    pub known_card_names: Vec<String>,
    /// Exact pasted list (empty when not provided).
    #[serde(default)]
    pub list_entries: Vec<DeckListEntry>,
    #[serde(default)]
    pub list_total_cards: u32,
}

/// Detailed, deck-specific strategy brief for the HUD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckStrategyBrief {
    pub matchup: String,
    pub your_plan: String,
    pub vs_opponent: String,
    pub this_turn: Vec<String>,
    pub threats: Vec<String>,
    pub priorities: Vec<String>,
    /// Card-specific lines derived from your list (empty if none).
    #[serde(default)]
    pub list_notes: Vec<String>,
    pub refreshed_at: String,
}

/// The data a brief is built from. Any of it can be missing.
pub struct StrategyContext<'a> {
    pub repo: &'a CardRepository<'a>,
    pub you: Option<&'a LeaderStats>,
    pub opponent: Option<&'a LeaderStats>,
    pub comparison: Option<&'a DeckComparison>,
}

/// Builds the plan text from data.
pub struct DeckStrategyCoach;

impl DeckStrategyCoach {
    pub fn brief(
        state: &GameState,
        you: &DeckProfile,
        opp: &DeckProfile,
        ctx: &StrategyContext<'_>,
    ) -> DeckStrategyBrief {
        DeckStrategyBrief {
            matchup: format!("{} vs {}", display_name(you), display_name(opp)),
            your_plan: your_plan(you, ctx),
            vs_opponent: vs_opponent(state, opp, ctx),
            this_turn: this_turn(state, ctx),
            threats: threats(opp, ctx),
            priorities: priorities(state, ctx),
            list_notes: list_notes(ctx),
            refreshed_at: now_label(),
        }
    }
}

fn display_name(d: &DeckProfile) -> String {
    if !d.name.trim().is_empty() && d.name != "Deck unknown" {
        d.name.clone()
    } else if !d.leader_name.is_empty() && d.leader_name != "Unknown leader" {
        if d.leader_color.is_empty() {
            d.leader_name.clone()
        } else {
            format!("{} {}", d.leader_color, d.leader_name)
        }
    } else if !d.leader_id.is_empty() {
        d.leader_id.clone()
    } else {
        "Unknown deck".into()
    }
}

fn names(cards: &[CoreCard], n: usize, lists: usize) -> String {
    cards
        .iter()
        .take(n)
        .map(|c| format!("{} ({} of {})", c.name, c.in_lists, lists))
        .collect::<Vec<_>>()
        .join(", ")
}

fn one_decimal(x: f32) -> String {
    let rounded = (x * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

/// "about 4 Blockers, 9 Counter cards" from the averages worth mentioning.
fn habits(stats: &LeaderStats) -> String {
    let mut parts = Vec::new();
    let mut add = |avg: f32, label: &str| {
        if avg >= 0.5 {
            parts.push(format!("{} {label}", one_decimal(avg)));
        }
    };
    add(stats.avg_counter_cards, "Counter cards");
    add(stats.avg_blockers, "Blockers");
    add(stats.avg_rush, "Rush cards");
    add(stats.avg_double_attack, "Double Attack cards");
    add(stats.avg_banish, "Banish cards");
    add(stats.avg_ko_effects, "cards with a K.O. effect");
    add(stats.avg_bounce, "cards that return Characters to hand");
    parts.join(", ")
}

fn your_plan(you: &DeckProfile, ctx: &StrategyContext<'_>) -> String {
    let Some(stats) = ctx.you else {
        return if you.leader_id.is_empty() {
            "Waiting to see your Leader in a game.".into()
        } else {
            format!(
                "No winning-list data for {} yet, so there is no plan to show. \
                 Run the winning-deck download to add it.",
                display_name(you)
            )
        };
    };
    if !stats.enough_data {
        return format!(
            "Only {} winning list{} for {} in the data: too few to describe how it is played.",
            stats.lists,
            if stats.lists == 1 { "" } else { "s" },
            stats.leader_name
        );
    }
    let mut parts = vec![format!(
        "{} recent winning {} lists ({}):",
        stats.lists, stats.leader_name, stats.format
    )];
    if let Some((t, avg)) = stats.top_types.first() {
        parts.push(format!("built around {t} (about {} of 50 cards).", avg.round()));
    }
    let habits = habits(stats);
    if !habits.is_empty() {
        parts.push(format!("They run about {habits}."));
    }
    if !stats.core.is_empty() {
        parts.push(format!("Core cards: {}.", names(&stats.core, 8, stats.lists)));
    }
    parts.join(" ")
}

fn vs_opponent(state: &GameState, opp: &DeckProfile, ctx: &StrategyContext<'_>) -> String {
    let Some(stats) = ctx.opponent else {
        return if opp.leader_id.is_empty() {
            "Waiting to see the opponent's Leader.".into()
        } else {
            format!(
                "No winning-list data for the opponent's Leader ({}).",
                display_name(opp)
            )
        };
    };
    if !stats.enough_data {
        return format!(
            "Only {} winning list{} for {} in the data: too few to say what they run.",
            stats.lists,
            if stats.lists == 1 { "" } else { "s" },
            stats.leader_name
        );
    }
    let mut parts = vec![format!(
        "Winning {} lists ({} lists, {}) run about {}.",
        stats.leader_name,
        stats.lists,
        stats.format,
        habits(stats)
    )];
    let hand = state.player_two().hand_count;
    if hand > 0 && stats.avg_counter_cards > 0.0 {
        // Their lists are 50 cards, so this is a density, not a count in hand.
        let expected = hand as f32 * stats.avg_counter_cards / 50.0;
        parts.push(format!(
            "With {hand} cards in their hand, about {} may be Counter cards (a rough estimate from their lists, not a read of their hand).",
            one_decimal(expected)
        ));
    }
    if !stats.core.is_empty() {
        parts.push(format!("Their core cards: {}.", names(&stats.core, 6, stats.lists)));
    }
    parts.join(" ")
}

fn card_line(id: &str, repo: &CardRepository<'_>) -> Option<(String, u32, CardType, Vec<&'static str>, i32)> {
    let def = repo.get_by_id(id).ok()?;
    let tags = card_tags(&def);
    Some((def.name.clone(), def.cost, def.card_type, tags, def.counter))
}

/// Facts about right now: DON!!, what is in hand, what is ready.
fn this_turn(state: &GameState, ctx: &StrategyContext<'_>) -> Vec<String> {
    let me = state.player_one();
    let opp = state.player_two();
    let mut steps = Vec::new();

    // The opponent's turn: what you hold to answer with.
    if state.active_player != 0 {
        steps.push("It is the opponent's turn.".into());
        let counters: i32 = me
            .hand
            .iter()
            .filter_map(|c| card_line(&c.card_id, ctx.repo))
            .map(|(_, _, _, _, counter)| counter.max(0))
            .sum();
        let counter_cards = me
            .hand
            .iter()
            .filter(|c| card_line(&c.card_id, ctx.repo).is_some_and(|l| l.4 > 0))
            .count();
        if counter_cards > 0 {
            steps.push(format!(
                "You hold {counter_cards} Counter card{} worth +{counters} in total.",
                if counter_cards == 1 { "" } else { "s" }
            ));
        }
        let blockers: Vec<String> = me
            .characters
            .iter()
            .filter(|c| !c.rested)
            .filter_map(|c| card_line(&c.card_id, ctx.repo))
            .filter(|l| l.3.contains(&"Blocker"))
            .map(|l| l.0)
            .collect();
        if !blockers.is_empty() {
            steps.push(format!("Ready Blockers: {}.", blockers.join(", ")));
        }
        return steps;
    }

    match state.phase {
        Phase::Draw => steps.push("Draw phase: draw a card, then you get DON!!.".into()),
        Phase::Don => steps.push("DON!! phase: you add DON!!, then the Main Phase starts.".into()),
        Phase::End => steps.push("Your turn is ending.".into()),
        Phase::Combat => {
            if state.combat.active {
                steps.push("A battle is open: see the combat panel for the numbers.".into());
            }
        }
        Phase::Main => {}
    }

    if matches!(state.phase, Phase::Main | Phase::Don) {
        steps.push(format!("You have {} active DON!!.", me.don_active));
        let mut playable: Vec<String> = Vec::new();
        for card in &me.hand {
            if let Some((name, cost, kind, tags, _)) = card_line(&card.card_id, ctx.repo) {
                if matches!(kind, CardType::Leader) || cost > me.don_active {
                    continue;
                }
                let label = if tags.is_empty() {
                    format!("{name} (cost {cost})")
                } else {
                    format!("{name} (cost {cost}, {})", tags.join(", "))
                };
                if !playable.contains(&label) {
                    playable.push(label);
                }
            }
        }
        if playable.is_empty() && !me.hand.is_empty() {
            steps.push("Nothing in your hand costs this little or less.".into());
        } else if !playable.is_empty() {
            steps.push(format!("You can play now: {}.", playable.join("; ")));
        }
        let ready = me.characters.iter().filter(|c| !c.rested).count();
        if ready > 0 {
            steps.push(format!(
                "{ready} of your Characters {} ready to attack.",
                if ready == 1 { "is" } else { "are" }
            ));
        }
        let their_blockers: Vec<String> = opp
            .characters
            .iter()
            .filter(|c| !c.rested)
            .filter_map(|c| card_line(&c.card_id, ctx.repo))
            .filter(|l| l.3.contains(&"Blocker"))
            .map(|l| l.0)
            .collect();
        if !their_blockers.is_empty() {
            steps.push(format!("Their ready Blockers: {}.", their_blockers.join(", ")));
        }
        steps.push(format!("Opponent Life: {}.", opp.life));
    }
    steps
}

fn threats(opp: &DeckProfile, ctx: &StrategyContext<'_>) -> Vec<String> {
    let mut out = Vec::new();
    // Cards they have actually shown that do something notable.
    for id in opp.known_card_ids.iter().take(40) {
        if let Some((name, _, _, tags, _)) = card_line(id, ctx.repo) {
            if !tags.is_empty() {
                out.push(format!("Seen: {name} ({})", tags.join(", ")));
            }
        }
    }
    // Notable cards their winning lists run that have not appeared yet.
    if let Some(stats) = ctx.opponent.filter(|s| s.enough_data) {
        for card in &stats.notable {
            if opp.known_card_ids.iter().any(|id| id == &card.card_id) {
                continue;
            }
            out.push(format!(
                "Not seen yet: {} ({}), in {} of {} of their winning lists",
                card.name,
                card.tags.join(", "),
                card.in_lists,
                stats.lists
            ));
            if out.len() >= 8 {
                break;
            }
        }
    }
    if out.is_empty() {
        out.push("No notable opponent cards seen yet.".into());
    }
    out
}

fn priorities(state: &GameState, ctx: &StrategyContext<'_>) -> Vec<String> {
    let me = state.player_one();
    let opp = state.player_two();
    let mut out = vec![
        format!("Life: you {} · opponent {}.", me.life, opp.life),
        format!(
            "Characters in play: you {} · opponent {}.",
            me.characters.len(),
            opp.characters.len()
        ),
    ];
    match ctx.comparison {
        Some(c) if c.has_list && c.enough_data => out.push(format!(
            "Counter cards: your list has {}, winning lists average {}.",
            c.your_counter_cards,
            one_decimal(c.avg_counter_cards)
        )),
        Some(c) if !c.has_list && c.enough_data => {
            out.push("Add your deck list to compare it with the winners.".into());
        }
        _ => {}
    }
    out
}

fn list_notes(ctx: &StrategyContext<'_>) -> Vec<String> {
    let Some(c) = ctx.comparison.filter(|c| c.has_list) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for v in &c.violations {
        out.push(format!("Not legal in Standard: {} ({}).", v.name, v.reason));
    }
    if !c.enough_data {
        return out;
    }
    for m in c.missing.iter().take(5) {
        out.push(format!(
            "Most winners also play {} ({} of {} lists).",
            m.name, m.in_lists, c.lists
        ));
    }
    for f in c.fewer_copies.iter().take(3) {
        out.push(format!(
            "{}: you run {}x, winners usually {}x.",
            f.name,
            f.your_copies,
            one_decimal(f.avg_copies)
        ));
    }
    for r in c.rarely_played.iter().take(3) {
        out.push(format!(
            "{} is unusual: only {} of {} winning lists run it.",
            r.name, r.in_lists, c.lists
        ));
    }
    out
}

fn now_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("t+{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leader_stats::leader_stats;
    use crate::meta::{MetaData, MetaDeck};
    use optcg_core::{CardInstance, Zone};
    use optcg_database::{AssetParser, Database};

    fn db() -> Database {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        db
    }

    fn profile(leader: &str, name: &str) -> DeckProfile {
        DeckProfile {
            name: name.into(),
            leader_id: leader.into(),
            leader_name: name.into(),
            ..Default::default()
        }
    }

    fn data() -> MetaData {
        MetaData {
            decks: (0..4)
                .map(|_| MetaDeck {
                    leader: "ST01-001".into(),
                    format: "OP17".into(),
                    cards: [("ST01-002".to_string(), 4u8), ("ST01-003".to_string(), 4u8)].into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn the_plan_comes_from_winning_lists_and_names_how_many() {
        let db = db();
        let repo = CardRepository::new(&db);
        let stats = leader_stats(&data(), &repo, None, "ST01-001", None);
        let ctx = StrategyContext { repo: &repo, you: Some(&stats), opponent: Some(&stats), comparison: None };
        let brief = DeckStrategyCoach::brief(
            &GameState::new(),
            &profile("ST01-001", "Luffy"),
            &profile("ST01-001", "Zoro"),
            &ctx,
        );
        assert!(brief.your_plan.contains("4 recent winning"), "{}", brief.your_plan);
        assert!(brief.your_plan.contains("Core cards"), "{}", brief.your_plan);
        assert_eq!(brief.matchup, "Luffy vs Zoro");
        assert!(brief.vs_opponent.contains("Winning"), "{}", brief.vs_opponent);
    }

    #[test]
    fn without_data_it_says_so_instead_of_guessing_by_colour() {
        let db = db();
        let repo = CardRepository::new(&db);
        let ctx = StrategyContext { repo: &repo, you: None, opponent: None, comparison: None };
        let mut you = profile("OP05-001", "Red Thing");
        you.leader_color = "Red".into();
        let brief = DeckStrategyCoach::brief(&GameState::new(), &you, &profile("OP05-002", "Other"), &ctx);
        assert!(brief.your_plan.contains("No winning-list data"), "{}", brief.your_plan);
        assert!(!brief.your_plan.to_lowercase().contains("aggro"), "no colour-based archetype guess");
        assert!(brief.vs_opponent.contains("No winning-list data"));
    }

    #[test]
    fn this_turn_lists_what_your_hand_can_actually_play() {
        let db = db();
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        state.phase = Phase::Main;
        state.player_one_mut().don_active = 2;
        state
            .player_one_mut()
            .hand
            .push(CardInstance::new("ST01-003", 0, Zone::Hand));
        let ctx = StrategyContext { repo: &repo, you: None, opponent: None, comparison: None };
        let brief = DeckStrategyCoach::brief(&state, &profile("ST01-001", "A"), &profile("ST01-001", "B"), &ctx);
        let text = brief.this_turn.join(" | ");
        assert!(text.contains("2 active DON!!"), "{text}");
        assert!(text.contains("You can play now"), "{text}");
    }

    #[test]
    fn the_opponents_turn_reports_what_you_hold_to_defend_with() {
        let db = db();
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        state.active_player = 1;
        state
            .player_one_mut()
            .hand
            .push(CardInstance::new("ST01-002", 0, Zone::Hand));
        let ctx = StrategyContext { repo: &repo, you: None, opponent: None, comparison: None };
        let brief = DeckStrategyCoach::brief(&state, &profile("ST01-001", "A"), &profile("ST01-001", "B"), &ctx);
        let text = brief.this_turn.join(" | ");
        assert!(text.contains("opponent's turn"), "{text}");
        assert!(text.contains("Counter card"), "{text}");
    }
}
