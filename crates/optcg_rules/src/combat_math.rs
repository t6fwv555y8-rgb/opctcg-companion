use optcg_core::GameState;
use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};

/// Structured combat calculation result (Milestone 2 foundation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatCalculation {
    pub attacker_power: i32,
    pub defender_power: i32,
    pub power_delta: i32,
    pub available_counter: i32,
    pub required_counter: i32,
    pub survives: bool,
}

/// Survival classification for HUD display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurvivalStatus {
    Survives,
    CounterRequired,
    Lethal,
}

/// What to do right now in the open battle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatDoThis {
    pub line: String,
    pub steps: Vec<String>,
}

/// Extended combat analysis for HUD (includes legacy fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatAnalysis {
    pub calculation: CombatCalculation,
    pub survival_status: SurvivalStatus,
    pub attacker_power: i32,
    pub defender_power: i32,
    pub power_differential: i32,
    pub required_counter: i32,
    pub survives_without_counter: bool,
    pub survives_with_base_counter: bool,
    pub lethal_to_leader: bool,
    pub blocker_available: bool,
    pub recommended_block: bool,
    pub shield_needed: u32,
}

/// Counter checker tracking variance differentials and survival thresholds.
pub struct CombatMath;

impl CombatMath {
    pub fn calculate(
        attacker_power: i32,
        defender_power: i32,
        available_counter: i32,
    ) -> CombatCalculation {
        let power_delta = attacker_power - defender_power;
        let required_counter = power_delta.max(0);
        let survives = power_delta <= available_counter;
        CombatCalculation {
            attacker_power,
            defender_power,
            power_delta,
            available_counter,
            required_counter,
            survives,
        }
    }

    pub fn survival_status(calc: &CombatCalculation, target_is_leader: bool) -> SurvivalStatus {
        if calc.power_delta <= 0 {
            SurvivalStatus::Survives
        } else if calc.survives {
            SurvivalStatus::Survives
        } else if target_is_leader && calc.power_delta > calc.available_counter {
            SurvivalStatus::Lethal
        } else {
            SurvivalStatus::CounterRequired
        }
    }

    pub fn analyze_attack(
        state: &GameState,
        repo: &CardRepository,
        attacker_id: &str,
        defender_id: Option<&str>,
        target_is_leader: bool,
    ) -> Option<CombatAnalysis> {
        let attacker_player = state
            .combat
            .attacker_player
            .map(|p| p as usize)
            .unwrap_or(state.active_player as usize);
        let defender_player = state
            .combat
            .target_player
            .map(|p| p as usize)
            .unwrap_or(1 - attacker_player);

        let attacker_power =
            Self::attacker_power(state, repo, attacker_id, attacker_player)?;

        let (defender_power, blocker_available, available_counter) = if target_is_leader {
            let leader = &state.players[defender_player];
            (
                leader.leader.effective_power() as i32,
                leader.find_blocker().is_some(),
                Self::counter_from_hand_estimate(leader.hand_count),
            )
        } else {
            let def_id = defender_id?;
            let defender = state.players[defender_player]
                .characters
                .iter()
                .find(|c| c.card_id == def_id)?;
            let defender_def = repo.get_by_id(def_id).ok()?;
            (
                defender.effective_power(defender_def.power),
                state.players[defender_player].find_blocker().is_some(),
                defender_def.keywords.counter_value(),
            )
        };

        let calculation = Self::calculate(attacker_power, defender_power, available_counter);
        let survival_status = Self::survival_status(&calculation, target_is_leader);
        let differential = calculation.power_delta;
        let required_counter = calculation.required_counter;
        let base_counter = if target_is_leader {
            available_counter
        } else {
            defender_id
                .and_then(|id| repo.get_by_id(id).ok())
                .map(|d| d.keywords.counter_value())
                .unwrap_or(0)
        };

        let survives_without = differential <= 0;
        let survives_with_base = differential <= base_counter;
        let lethal_to_leader = target_is_leader && differential > available_counter;

        let recommended_block = !survives_without && blocker_available && differential < 8000;

        info!(
            target: "optcg::combat",
            atk = attacker_power,
            def = defender_power,
            delta = differential,
            "combat calculated"
        );

        Some(CombatAnalysis {
            calculation: calculation.clone(),
            survival_status,
            attacker_power,
            defender_power,
            power_differential: differential,
            required_counter,
            survives_without_counter: survives_without,
            survives_with_base_counter: survives_with_base,
            lethal_to_leader,
            blocker_available,
            recommended_block,
            shield_needed: required_counter.max(0) as u32,
        })
    }

    pub fn analyze_current_combat(
        state: &GameState,
        repo: &CardRepository,
    ) -> Option<CombatAnalysis> {
        if !state.combat.active {
            return None;
        }
        let attacker = state.combat.attacker_id.as_deref()?;
        let target_is_leader = state.combat.target_is_leader;
        let defender = state.combat.target_id.as_deref();
        Self::analyze_attack(state, repo, attacker, defender, target_is_leader)
    }

    /// Imperative for the open swing — block, counter, take it, or go through.
    pub fn do_this(state: &GameState, analysis: Option<&CombatAnalysis>) -> Option<CombatDoThis> {
        if !state.combat.active && analysis.is_none() {
            return None;
        }

        let defending = you_are_defending(state, analysis);
        let target = target_label(state);

        if let Some(a) = analysis {
            let need = fmt_power(a.required_counter);
            if defending {
                if a.lethal_to_leader {
                    let mut steps = Vec::new();
                    if a.blocker_available || state.combat.blocker_offered {
                        steps.push("Block this swing with a ready character.".into());
                    }
                    if a.required_counter > 0 {
                        steps.push(format!("Or counter {need} to keep the life."));
                    }
                    steps.push("If you take it, you lose.".into());
                    let line = if a.blocker_available || state.combat.blocker_offered {
                        format!("Block this or counter {need} — this swing is lethal.")
                    } else {
                        format!("Counter {need} or you lose — this swing is lethal.")
                    };
                    return Some(CombatDoThis { line, steps });
                }
                if a.recommended_block || (state.combat.blocker_offered && !a.survives_without_counter)
                {
                    return Some(CombatDoThis {
                        line: format!("Block this swing at {target}."),
                        steps: vec![
                            "Activate a ready blocker.".into(),
                            if a.required_counter > 0 {
                                format!("If you don't block, you need {need} counter.")
                            } else {
                                "If you don't block, take the hit.".into()
                            },
                        ],
                    });
                }
                if a.required_counter > 0 && !a.survives_without_counter {
                    return Some(CombatDoThis {
                        line: format!("Counter {need} or take the hit on {target}."),
                        steps: vec![
                            format!("Play {need} counter if this body / life is worth the card."),
                            "Otherwise take the hit and keep the cards.".into(),
                        ],
                    });
                }
                if a.survives_without_counter {
                    return Some(CombatDoThis {
                        line: format!("This swing doesn't break {target} — take it."),
                        steps: vec![
                            "Don't spend a blocker or counter here.".into(),
                            "Resolve and get back to the turn.".into(),
                        ],
                    });
                }
            } else if a.lethal_to_leader {
                return Some(CombatDoThis {
                    line: format!("This swing is lethal on {target} — go through."),
                    steps: vec![
                        "Confirm the attack.".into(),
                        "Watch for a blocker before it resolves.".into(),
                    ],
                });
            } else if a.required_counter > 0 {
                return Some(CombatDoThis {
                    line: format!("Swing at {target}. They need {need} to live."),
                    steps: vec![
                        format!("Attack {target}."),
                        format!("They must counter {need} or lose the body / life."),
                    ],
                });
            } else {
                return Some(CombatDoThis {
                    line: format!("Swing at {target} — they don't break this."),
                    steps: vec![
                        format!("Attack {target}."),
                        "Resolve and continue the turn.".into(),
                    ],
                });
            }
        }

        if state.combat.blocker_offered {
            return Some(CombatDoThis {
                line: "Blocker window — decide now.".into(),
                steps: vec![
                    format!("They're swinging at {target}."),
                    "Block, counter, or take the hit.".into(),
                ],
            });
        }
        if defending {
            return Some(CombatDoThis {
                line: format!("They're swinging at {target} — block, counter, or take it."),
                steps: vec![
                    "Compare powers on this attack.".into(),
                    "Block or counter only if the save is worth the card.".into(),
                ],
            });
        }
        if state.combat.attacker_player == Some(0) || state.combat.active {
            return Some(CombatDoThis {
                line: format!("You're attacking {target} — resolve this swing."),
                steps: vec![
                    format!("Confirm the attack on {target}."),
                    "Watch for their blocker, then resolve.".into(),
                ],
            });
        }
        Some(CombatDoThis {
            line: "A battle is open — resolve this swing before anything else.".into(),
            steps: vec![
                "Identify the attacker and the target.".into(),
                "Decide block, counter, or take the hit.".into(),
            ],
        })
    }

    fn attacker_power(
        state: &GameState,
        repo: &CardRepository,
        attacker_id: &str,
        attacker_player: usize,
    ) -> Option<i32> {
        let player = state.players.get(attacker_player)?;
        if let Some(attacker) = player
            .characters
            .iter()
            .find(|c| c.card_id == attacker_id)
        {
            let attacker_def = repo.get_by_id(attacker_id).ok()?;
            return Some(attacker.effective_power(attacker_def.power));
        }
        let is_leader = attacker_id.eq_ignore_ascii_case("leader")
            || player.leader.card_id.eq_ignore_ascii_case(attacker_id);
        if is_leader {
            Some(player.leader.effective_power() as i32)
        } else {
            None
        }
    }

    pub fn counter_from_hand_estimate(hand_count: u32) -> i32 {
        (hand_count as i32).min(5) * 1000
    }
}

use optcg_core::CardDefinition;
use tracing::info;

fn fmt_power(n: i32) -> String {
    if n >= 1000 && n % 1000 == 0 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

fn target_label(state: &GameState) -> &'static str {
    if state.combat.target_is_leader {
        if state.combat.target_player == Some(0) {
            "your leader"
        } else {
            "their leader"
        }
    } else if state.combat.target_player == Some(0) {
        "your character"
    } else {
        "their character"
    }
}

fn you_are_defending(state: &GameState, analysis: Option<&CombatAnalysis>) -> bool {
    if state.combat.target_player == Some(0) {
        return true;
    }
    if state.combat.target_player == Some(1) {
        return false;
    }
    if state.combat.attacker_player == Some(1) {
        return true;
    }
    if state.combat.attacker_player == Some(0) {
        return false;
    }
    analysis.is_some_and(|a| {
        a.lethal_to_leader || a.recommended_block || a.required_counter > 0
    }) && state.combat.target_is_leader
}

impl CombatMath {
    pub fn optimal_counter_play(
        available_counters: &[CardDefinition],
        required: i32,
    ) -> Option<&CardDefinition> {
        available_counters
            .iter()
            .filter(|c| c.counter >= required)
            .min_by_key(|c| c.counter)
            .or_else(|| available_counters.iter().max_by_key(|c| c.counter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use optcg_core::CombatState;

    #[test]
    fn calculate_survives_with_sufficient_counter() {
        let calc = CombatMath::calculate(7000, 6000, 2000);
        assert_eq!(calc.power_delta, 1000);
        assert_eq!(calc.required_counter, 1000);
        assert!(calc.survives);
    }

    #[test]
    fn calculate_lethal_without_counter() {
        let calc = CombatMath::calculate(7000, 6000, 0);
        assert!(!calc.survives);
        assert_eq!(calc.required_counter, 1000);
    }

    #[test]
    fn survival_status_lethal_on_leader() {
        let calc = CombatMath::calculate(8000, 5000, 0);
        assert_eq!(
            CombatMath::survival_status(&calc, true),
            SurvivalStatus::Lethal
        );
    }

    fn combat_state(
        attacker_id: &str,
        attacker_player: u8,
        target_player: u8,
        target_is_leader: bool,
    ) -> CombatState {
        CombatState {
            active: true,
            attacker_id: Some(attacker_id.into()),
            attacker_player: Some(attacker_player),
            target_id: Some("leader".into()),
            target_player: Some(target_player),
            target_is_leader,
            ..CombatState::default()
        }
    }

    #[test]
    fn analyze_leader_attack_on_leader() {
        let db = optcg_database::Database::open_in_memory().unwrap();
        let _ = optcg_database::AssetParser::seed_defaults(&db);
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        state.combat = combat_state("ST01-001", 0, 1, true);
        let analysis = CombatMath::analyze_current_combat(&state, &repo).unwrap();
        assert_eq!(analysis.attacker_power, 5000);
        assert_eq!(analysis.defender_power, 5000);
        assert!(analysis.survives_without_counter);
    }

    #[test]
    fn analyze_character_attack_uses_board_body() {
        let db = optcg_database::Database::open_in_memory().unwrap();
        let _ = optcg_database::AssetParser::seed_defaults(&db);
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        state.players[0].characters.push(optcg_core::CardInstance::new(
            "ST01-012",
            0,
            optcg_core::Zone::Character,
        ));
        state.combat = combat_state("ST01-012", 0, 1, true);
        let analysis = CombatMath::analyze_current_combat(&state, &repo).unwrap();
        assert_eq!(analysis.attacker_power, 6000);
        assert_eq!(analysis.required_counter, 1000);
    }

    #[test]
    fn do_this_follows_lethal_defense() {
        let db = optcg_database::Database::open_in_memory().unwrap();
        let _ = optcg_database::AssetParser::seed_defaults(&db);
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        let mut sanji = optcg_core::CardInstance::new("ST01-012", 1, optcg_core::Zone::Character);
        sanji.attached_don = 3;
        state.players[1].characters.push(sanji);
        state.players[0].hand_count = 0;
        state.combat = combat_state("ST01-012", 1, 0, true);
        let analysis = CombatMath::analyze_current_combat(&state, &repo).unwrap();
        assert!(analysis.lethal_to_leader);
        let battle = CombatMath::do_this(&state, Some(&analysis)).unwrap();
        assert!(battle.line.to_lowercase().contains("lethal"));
        assert!(battle.line.to_lowercase().contains("counter") || battle.line.to_lowercase().contains("block"));
        assert!(!battle.steps.is_empty());
    }

    #[test]
    fn do_this_tells_you_to_go_through_when_your_swing_is_lethal() {
        let db = optcg_database::Database::open_in_memory().unwrap();
        let _ = optcg_database::AssetParser::seed_defaults(&db);
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        let mut sanji = optcg_core::CardInstance::new("ST01-012", 0, optcg_core::Zone::Character);
        sanji.attached_don = 3;
        state.players[0].characters.push(sanji);
        state.players[1].hand_count = 0;
        state.combat = combat_state("ST01-012", 0, 1, true);
        let analysis = CombatMath::analyze_current_combat(&state, &repo).unwrap();
        let battle = CombatMath::do_this(&state, Some(&analysis)).unwrap();
        assert!(battle.line.to_lowercase().contains("lethal"));
        assert!(battle.line.to_lowercase().contains("go through"));
    }

    #[test]
    fn do_this_uses_the_open_swing_without_math() {
        let mut state = GameState::new();
        state.combat = combat_state("ST01-001", 1, 0, true);
        let battle = CombatMath::do_this(&state, None).unwrap();
        assert!(battle.line.to_lowercase().contains("swinging"));
        assert!(battle.line.to_lowercase().contains("your leader"));
    }
}
