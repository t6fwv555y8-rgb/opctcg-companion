use crate::deck_list::DeckListEntry;
use optcg_core::{GameState, Phase};
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
    /// Card-specific lines derived from the pasted list (empty if none).
    #[serde(default)]
    pub list_notes: Vec<String>,
    pub refreshed_at: String,
}

/// Builds human-readable strategy text tailored to the decks in play.
pub struct DeckStrategyCoach;

impl DeckStrategyCoach {
    pub fn brief(
        state: &GameState,
        you: &DeckProfile,
        opp: &DeckProfile,
    ) -> DeckStrategyBrief {
        let matchup = format!(
            "{} vs {}",
            display_name(you),
            display_name(opp)
        );
        let your_plan = your_game_plan(you);
        let vs_opponent = matchup_plan(you, opp);
        let this_turn = turn_priorities(state, you, opp);
        let threats = opponent_threats(opp);
        let priorities = overall_priorities(you, opp, state);
        let list_notes = list_specific_notes(you);

        DeckStrategyBrief {
            matchup,
            your_plan,
            vs_opponent,
            this_turn,
            threats,
            priorities,
            list_notes,
            refreshed_at: chrono_now(),
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

fn color_key(d: &DeckProfile) -> String {
    d.leader_color.trim().to_ascii_lowercase()
}

fn archetype_hint(d: &DeckProfile) -> &'static str {
    let name = format!(
        "{} {} {}",
        d.name.to_ascii_lowercase(),
        d.leader_name.to_ascii_lowercase(),
        d.leader_id.to_ascii_lowercase()
    );
    let color = color_key(d);

    // Exact-list packages
    if d.list_entries.iter().any(|e| e.card_id == "OP17-079")
        || d.leader_id.eq_ignore_ascii_case("OP17-079")
    {
        return "elbaph";
    }

    if name.contains("aggro") || name.contains("rush") {
        return "aggro";
    }
    if name.contains("control") || name.contains("midrange") {
        return "control";
    }
    if name.contains("combo") || name.contains("otk") {
        return "combo";
    }

    match color.as_str() {
        "red" => "aggro",
        "green" => "ramp",
        "blue" => "control",
        "purple" => "combo",
        "black" => "removal",
        "yellow" => "life",
        _ => "midrange",
    }
}

fn your_game_plan(you: &DeckProfile) -> String {
    let label = display_name(you);
    let arch = archetype_hint(you);
    let color = color_key(you);
    let mut parts = vec![format!("{label} game plan ({arch}):")];

    match arch {
        "elbaph" => parts.push(
            "Black Elbaph Luffy plan: establish a 12+ cost Character (Saul +12 / Loki +12), then turn on the package — Leader Blocker on 12+ bodies, board buffs (+3000), Usopp/Saul Elbaph searches, and Rush Luffy (OP17-093) for the kill."
                .into(),
        ),
        "aggro" => parts.push(
            "Curve out early characters, attach DON to push leader/character swings, and race life before the opponent stabilizes."
                .into(),
        ),
        "control" => parts.push(
            "Trade efficiently, protect life with counters/blockers, and convert late-game resources once the board is clean."
                .into(),
        ),
        "ramp" => parts.push(
            "Accelerate DON, develop medium-cost bodies, then overwhelm with larger plays once ahead on resources."
                .into(),
        ),
        "combo" => parts.push(
            "Survive early pressure, assemble key pieces, and sequence your combo turn carefully — don't overextend into removal."
                .into(),
        ),
        "removal" => parts.push(
            "Answer opposing threats on curve, keep life healthy, then close with sticky midrange attackers."
                .into(),
        ),
        "life" => parts.push(
            "Manage life as a resource, leverage life-gain / life-pay effects, and time lethal when the opponent is low."
                .into(),
        ),
        _ => parts.push(
            "Develop on curve, contest board, and look for profitable attacks when ahead on power or life."
                .into(),
        ),
    }

    if color == "red" {
        parts.push("Red tip: prefer aggressive DON attachment and direct leader pressure.".into());
    } else if color == "blue" {
        parts.push("Blue tip: hold counters for lethal turns; don't over-commit early.".into());
    } else if color == "green" {
        parts.push("Green tip: value DON ramp; don't waste early turns if you can accelerate.".into());
    } else if color == "purple" {
        parts.push("Purple tip: trash/cost reduction matters — track key pieces in hand/trash.".into());
    } else if color == "black" {
        parts.push("Black tip: keep removal for high-value targets, not weak early bodies.".into());
    } else if color == "yellow" {
        parts.push("Yellow tip: watch life totals closely; your triggers can swing races.".into());
    }

    if !you.list_entries.is_empty() {
        parts.push(list_composition_summary(you));
        if let Some(key) = key_finishers(you).into_iter().next() {
            parts.push(format!("Key closer in list: {key}."));
        }
    } else if !you.known_card_names.is_empty() {
        let sample: Vec<_> = you.known_card_names.iter().take(4).cloned().collect();
        parts.push(format!(
            "Cards seen in your list so far: {}.",
            sample.join(", ")
        ));
    }

    parts.join(" ")
}

fn list_composition_summary(you: &DeckProfile) -> String {
    let total = you.list_total_cards.max(
        you.list_entries
            .iter()
            .map(|e| u32::from(e.quantity))
            .sum(),
    );
    let blockers: u32 = you
        .list_entries
        .iter()
        .filter(|e| e.blocker)
        .map(|e| u32::from(e.quantity))
        .sum();
    let rush: u32 = you
        .list_entries
        .iter()
        .filter(|e| e.rush)
        .map(|e| u32::from(e.quantity))
        .sum();
    let counters: u32 = you
        .list_entries
        .iter()
        .filter(|e| e.counter > 0)
        .map(|e| u32::from(e.quantity))
        .sum();
    let low: u32 = you
        .list_entries
        .iter()
        .filter(|e| e.card_type != "leader" && e.cost <= 2)
        .map(|e| u32::from(e.quantity))
        .sum();
    let mid: u32 = you
        .list_entries
        .iter()
        .filter(|e| e.cost >= 3 && e.cost <= 5)
        .map(|e| u32::from(e.quantity))
        .sum();
    let high: u32 = you
        .list_entries
        .iter()
        .filter(|e| e.cost >= 6)
        .map(|e| u32::from(e.quantity))
        .sum();

    format!(
        "Your pasted list ({total} cards): curve low≤2:{low} / mid3–5:{mid} / high6+:{high}; blockers×{blockers}, rush×{rush}, counter cards×{counters}."
    )
}

fn key_finishers(you: &DeckProfile) -> Vec<String> {
    let mut scored: Vec<(i32, String)> = you
        .list_entries
        .iter()
        .filter(|e| e.card_type != "leader")
        .map(|e| {
            let mut s = e.cost as i32 * 10 + e.quantity as i32;
            if e.rush {
                s += 40;
            }
            if e.blocker {
                s += 15;
            }
            (
                s,
                format!("{}×{} (cost {})", e.quantity, e.name, e.cost),
            )
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(3).map(|(_, s)| s).collect()
}

fn list_specific_notes(you: &DeckProfile) -> Vec<String> {
    if you.list_entries.is_empty() {
        return vec![
            "Paste your exact deck list for card-by-card lines (4x ID / Leader: / Deck: formats)."
                .into(),
        ];
    }

    let mut notes = Vec::new();
    notes.push(list_composition_summary(you));

    // Black Elbaph Luffy (OP17-079) package — exact-list coaching
    let is_elbaph = you.list_entries.iter().any(|e| e.card_id == "OP17-079")
        || you.leader_id.eq_ignore_ascii_case("OP17-079");
    if is_elbaph {
        notes.push(
            "Win condition: get ANY Character to effective cost 12+ (Saul +12 / Loki +12), which turns on Leader Blocker + Straw Hat pumps/Rush."
                .into(),
        );
        if let Some(saul) = you.list_entries.iter().find(|e| e.card_id == "OP17-089") {
            notes.push(format!(
                "{}× Jaguar.D.Saul — primary 12+ enabler (+12 cost) and Elbaph dig; prioritize landing him by turn 4–5.",
                saul.quantity
            ));
        }
        if let Some(loki) = you.list_entries.iter().find(|e| e.card_id == "OP17-119") {
            notes.push(format!(
                "{}× Loki — secondary enabler (+12 cost), K.O. ≤4 total cost, 11k on their turn.",
                loki.quantity
            ));
        }
        if let Some(luffy) = you.list_entries.iter().find(|e| e.card_id == "OP17-093") {
            notes.push(format!(
                "{}× Luffy (8c) — Rush if 12+ is out; On Play recycles ≤2 from trash. Drop him the turn Saul/Loki sticks.",
                luffy.quantity
            ));
        }
        if let Some(usopp) = you.list_entries.iter().find(|e| e.card_id == "OP17-080") {
            notes.push(format!(
                "{}× Usopp — Elbaph search + Counter 1000; becomes 5k with 12+ online.",
                usopp.quantity
            ));
        }
        if let Some(kong) = you.list_entries.iter().find(|e| e.card_id == "OP17-098") {
            notes.push(format!(
                "{}× Gum-Gum Kong Gun — Main clear (rest 6 DON, need 12+) or Leader Counter +3000.",
                kong.quantity
            ));
        }
        if let Some(patch) = you.list_entries.iter().find(|e| e.card_id == "OP05-094") {
            notes.push(format!(
                "{}× Haute Couture Patch★Work — −3 cost / freeze; shrink blockers before a pumped swing.",
                patch.quantity
            ));
        }
        notes.push(
            "Curve: flood 1–2 drops early, dig with Usopp/Saul, slam 12+, then convert with Rush Luffy / Kong Gun."
                .into(),
        );
        return notes;
    }

    for e in you
        .list_entries
        .iter()
        .filter(|e| e.rush)
        .take(3)
    {
        notes.push(format!(
            "{}×{} has Rush — keep DON ready to swing the turn it lands.",
            e.quantity, e.name
        ));
    }
    for e in you
        .list_entries
        .iter()
        .filter(|e| e.blocker)
        .take(3)
    {
        notes.push(format!(
            "{}×{} is a Blocker — hold for lethal turns or vs tall attackers.",
            e.quantity, e.name
        ));
    }
    for e in you
        .list_entries
        .iter()
        .filter(|e| e.counter >= 2000)
        .take(3)
    {
        notes.push(format!(
            "{}×{} offers Counter +{} — save for lethal defense.",
            e.quantity, e.name, e.counter
        ));
    }

    // Curve advice from exact counts
    let twos: u32 = you
        .list_entries
        .iter()
        .filter(|e| e.cost == 2)
        .map(|e| u32::from(e.quantity))
        .sum();
    let fives: u32 = you
        .list_entries
        .iter()
        .filter(|e| e.cost == 5)
        .map(|e| u32::from(e.quantity))
        .sum();
    if twos >= 6 {
        notes.push(format!(
            "Heavy 2-drops ({twos}) — prioritize early board presence every turn 2–3."
        ));
    }
    if fives >= 4 {
        notes.push(format!(
            "Strong 5-drop density ({fives}) — bank DON so you can deploy on curve."
        ));
    }

    for finisher in key_finishers(you).into_iter().take(2) {
        notes.push(format!("Pilot around {finisher}."));
    }

    notes
}

fn matchup_plan(you: &DeckProfile, opp: &DeckProfile) -> String {
    let you_a = archetype_hint(you);
    let opp_a = archetype_hint(opp);
    let opp_label = display_name(opp);

    let core = match (you_a, opp_a) {
        ("elbaph", _) => {
            format!(
                "Vs {opp_label}: race to a 12+ cost piece (4× Saul or 4× Loki). Once it's online, your low-cost Straw Hats pump, Leader gives Blocker to 12+ bodies, and 4× Luffy (OP17-093) can Rush for lethal. Use Patch★Work / Kong Gun to clear blockers before the swing."
            )
        }
        ("aggro", "control") => {
            format!(
                "Vs {opp_label} (control): race hard. Force them to spend counters early, attack leader every turn you can, and don't slow-roll development."
            )
        }
        ("aggro", "aggro") => {
            format!(
                "Vs {opp_label} (mirror aggro): race math matters. Protect life when behind on board, and swing for lethal the turn you go ahead on power."
            )
        }
        ("control", "aggro") => {
            format!(
                "Vs {opp_label} (aggro): stabilize first. Block/counter key swings, clear their early board, then take over once their gas runs out."
            )
        }
        ("control", "control") => {
            format!(
                "Vs {opp_label} (control mirror): value and patience. Don't dump hand into bad trades; win the late resource war."
            )
        }
        ("ramp", "aggro") => {
            format!(
                "Vs {opp_label} (aggro): survive the rush. Use blockers/life carefully while you ramp, then drop larger bodies once stable."
            )
        }
        ("combo", _) => {
            format!(
                "Vs {opp_label}: buy turns. Keep life above lethal range, assemble combo pieces, and only go off when you can finish or protect the line."
            )
        }
        ("removal", "aggro") | ("removal", "midrange") => {
            format!(
                "Vs {opp_label}: remove their best attacker each turn and keep pressure modest until you stabilize."
            )
        }
        ("life", "aggro") => {
            format!(
                "Vs {opp_label} (aggro): your life buffer is the plan — absorb early hits, then convert life advantage into a winning race."
            )
        }
        _ => {
            format!(
                "Vs {opp_label} ({opp_a}): play to your {you_a} strengths — contest board, track life totals, and only commit DON when the attack is profitable."
            )
        }
    };

    let mut out = core;
    if !opp.known_card_names.is_empty() {
        let seen: Vec<_> = opp.known_card_names.iter().take(5).cloned().collect();
        out.push_str(&format!(
            " Opponent cards observed: {} — play around those lines.",
            seen.join(", ")
        ));
    }
    out
}

fn turn_priorities(state: &GameState, you: &DeckProfile, _opp: &DeckProfile) -> Vec<String> {
    if state.combat.active {
        if let Some(battle) = crate::combat_math::CombatMath::do_this(state, None, None) {
            return battle.steps;
        }
    }
    let you_p = &state.players[state.active_player as usize];
    let opp_p = &state.players[(1 - state.active_player) as usize];
    let arch = archetype_hint(you);
    let mut steps = Vec::new();

    match state.phase {
        Phase::Draw => {
            steps.push("Draw for turn, then move to DON with a plan for attachments.".into());
        }
        Phase::Don => {
            if you_p.don_active > 0 {
                if arch == "aggro" {
                    steps.push(format!(
                        "Attach {} DON toward lethal pressure (leader or your best attacker).",
                        you_p.don_active
                    ));
                } else if opp_p.characters.iter().any(|c| !c.rested) {
                    steps.push(
                        "Attach DON to match or beat their active threat before Main attacks."
                            .into(),
                    );
                } else {
                    steps.push(format!(
                        "Bank or attach {} active DON for your planned Main play.",
                        you_p.don_active
                    ));
                }
            } else {
                steps.push("No active DON — advance to Main and develop/attack.".into());
            }
        }
        Phase::Main => {
            if state.combat.active {
                steps.push("Resolve the open combat first (block/counter), then continue Main."
                    .into());
            } else if you_p.characters.is_empty() {
                steps.push(format!(
                    "{}: develop at least one character before attacking.",
                    display_name(you)
                ));
            } else if opp_p.life <= 2 {
                steps.push(
                    "Opponent is low life — calculate leader attacks for lethal / near-lethal."
                        .into(),
                );
            } else if arch == "control" && !opp_p.characters.is_empty() {
                steps.push(
                    "Clear or neutralize their board before racing; don't gift them free trades."
                        .into(),
                );
            } else {
                steps.push(
                    "Play your best affordable character, then attack with active units / leader."
                        .into(),
                );
            }
            if you_p.don_active >= 2 && arch == "aggro" {
                steps.push("Keep enough DON attached to win the key swing this turn.".into());
            }
            // Exact-list affordability hints
            if !you.list_entries.is_empty() {
                let don = you_p.don_active + you_p.don_rested;
                let plays: Vec<_> = you
                    .list_entries
                    .iter()
                    .filter(|e| e.card_type == "character" && e.cost > 0 && e.cost <= don)
                    .take(3)
                    .map(|e| format!("{} (c{})", e.name, e.cost))
                    .collect();
                if !plays.is_empty() {
                    steps.push(format!(
                        "From your list, curve options at ≤{don} DON: {}.",
                        plays.join(", ")
                    ));
                }
                if let Some(rush) = you.list_entries.iter().find(|e| e.rush && e.cost <= don) {
                    steps.push(format!(
                        "If you have {} in hand, Rush swing is on-curve this turn.",
                        rush.name
                    ));
                }
            }
        }
        Phase::Combat => {
            if state.combat.blocker_offered {
                steps.push("Decide blocker vs take life — preserve lethal math next turn.".into());
            } else {
                steps.push("Compare powers; counter only if the life save is worth the card."
                    .into());
            }
            if let Some(b) = you.list_entries.iter().find(|e| e.blocker) {
                steps.push(format!(
                    "Your list includes Blocker {} — use it if this hit is lethal.",
                    b.name
                ));
            }
        }
        Phase::End => {
            steps.push("End cleanly; note remaining DON and known opponent threats.".into());
        }
    }

    if you_p.life + 1 < opp_p.life {
        steps.push("You are behind on life — prioritize pressure over slow development.".into());
    }

    steps
}

fn opponent_threats(opp: &DeckProfile) -> Vec<String> {
    let mut threats = Vec::new();
    let arch = archetype_hint(opp);
    threats.push(format!(
        "{} plays like a {} deck — expect {}.",
        display_name(opp),
        arch,
        match arch {
            "aggro" => "early swings and DON-stacked attacks",
            "control" => "counters, answers, and late-game value",
            "ramp" => "accelerated high-cost plays",
            "combo" => "a burst turn once pieces assemble",
            "removal" => "targeted KO / bounce on your best body",
            "life" => "life manipulation and trigger swings",
            _ => "standard midrange board contests",
        }
    ));

    for name in opp.known_card_names.iter().take(6) {
        threats.push(format!("Seen: {name} — respect it in combat/math."));
    }

    if threats.len() == 1 {
        threats.push("Few opponent cards identified yet — refresh again as more appear.".into());
    }
    threats
}

fn overall_priorities(you: &DeckProfile, opp: &DeckProfile, state: &GameState) -> Vec<String> {
    let you_p = state.player_one();
    let opp_p = state.player_two();
    let mut p = Vec::new();
    p.push(format!("Play your {} plan; don't drift into their pace.", archetype_hint(you)));
    p.push(format!(
        "Matchup focus: {}",
        matchup_plan(you, opp)
            .split(". ")
            .next()
            .unwrap_or("contest board and life")
    ));
    p.push(format!(
        "Life check: you {} — opp {}.",
        you_p.life, opp_p.life
    ));
    p.push(format!(
        "Board: you {} characters · opp {} characters.",
        you_p.characters.len(),
        opp_p.characters.len()
    ));
    p
}

fn chrono_now() -> String {
    // Keep dependency-light: RFC3339-ish from system time if chrono unavailable in this crate.
    // optcg_core already uses chrono; rules crate may not — use simple UTC via std if needed.
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
    use optcg_core::GameState;

    #[test]
    fn brief_mentions_both_decks() {
        let state = GameState::new();
        let you = DeckProfile {
            name: "Red Luffy Aggro".into(),
            leader_id: "ST01-001".into(),
            leader_name: "Monkey.D.Luffy".into(),
            leader_color: "Red".into(),
            known_card_ids: vec!["ST01-002".into()],
            known_card_names: vec!["Usopp".into()],
            list_entries: vec![],
            list_total_cards: 0,
        };
        let opp = DeckProfile {
            name: "Blue Control".into(),
            leader_id: "OP01-001".into(),
            leader_name: "Trafalgar Law".into(),
            leader_color: "Blue".into(),
            known_card_ids: vec![],
            known_card_names: vec![],
            list_entries: vec![],
            list_total_cards: 0,
        };
        let brief = DeckStrategyCoach::brief(&state, &you, &opp);
        assert!(brief.matchup.contains("Red Luffy Aggro"));
        assert!(brief.matchup.contains("Blue Control"));
        assert!(!brief.your_plan.is_empty());
        assert!(!brief.vs_opponent.is_empty());
        assert!(!brief.this_turn.is_empty());
        assert!(!brief.priorities.is_empty());
        assert!(!brief.list_notes.is_empty());
    }

    #[test]
    fn brief_uses_pasted_list_notes() {
        use crate::deck_list::DeckListEntry;
        let state = GameState::new();
        let you = DeckProfile {
            name: "Red Luffy Aggro".into(),
            leader_id: "ST01-001".into(),
            leader_name: "Monkey.D.Luffy".into(),
            leader_color: "Red".into(),
            known_card_ids: vec![],
            known_card_names: vec![],
            list_entries: vec![
                DeckListEntry {
                    card_id: "ST01-012".into(),
                    name: "Sanji".into(),
                    quantity: 4,
                    cost: 5,
                    card_type: "character".into(),
                    color: "Red".into(),
                    rush: true,
                    blocker: false,
                    counter: 0,
                },
                DeckListEntry {
                    card_id: "ST01-010".into(),
                    name: "Nami".into(),
                    quantity: 2,
                    cost: 3,
                    card_type: "character".into(),
                    color: "Red".into(),
                    rush: false,
                    blocker: true,
                    counter: 0,
                },
            ],
            list_total_cards: 6,
        };
        let opp = DeckProfile::default();
        let brief = DeckStrategyCoach::brief(&state, &you, &opp);
        assert!(brief.your_plan.contains("pasted list") || brief.list_notes.iter().any(|n| n.contains("Rush") || n.contains("Sanji")));
        assert!(brief.list_notes.iter().any(|n| n.contains("Blocker") || n.contains("Nami")));
    }
}
