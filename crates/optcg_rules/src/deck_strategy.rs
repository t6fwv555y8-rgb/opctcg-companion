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

        DeckStrategyBrief {
            matchup,
            your_plan,
            vs_opponent,
            this_turn,
            threats,
            priorities,
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

    if !you.known_card_names.is_empty() {
        let sample: Vec<_> = you.known_card_names.iter().take(4).cloned().collect();
        parts.push(format!(
            "Cards seen in your list so far: {}.",
            sample.join(", ")
        ));
    }

    parts.join(" ")
}

fn matchup_plan(you: &DeckProfile, opp: &DeckProfile) -> String {
    let you_a = archetype_hint(you);
    let opp_a = archetype_hint(opp);
    let opp_label = display_name(opp);

    let core = match (you_a, opp_a) {
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
        }
        Phase::Combat => {
            if state.combat.blocker_offered {
                steps.push("Decide blocker vs take life — preserve lethal math next turn.".into());
            } else {
                steps.push("Compare powers; counter only if the life save is worth the card."
                    .into());
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
        };
        let opp = DeckProfile {
            name: "Blue Control".into(),
            leader_id: "OP01-001".into(),
            leader_name: "Trafalgar Law".into(),
            leader_color: "Blue".into(),
            known_card_ids: vec![],
            known_card_names: vec![],
        };
        let brief = DeckStrategyCoach::brief(&state, &you, &opp);
        assert!(brief.matchup.contains("Red Luffy Aggro"));
        assert!(brief.matchup.contains("Blue Control"));
        assert!(!brief.your_plan.is_empty());
        assert!(!brief.vs_opponent.is_empty());
        assert!(!brief.this_turn.is_empty());
        assert!(!brief.priorities.is_empty());
    }
}
