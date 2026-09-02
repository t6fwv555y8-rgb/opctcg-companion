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
        let attacker_player = state.active_player as usize;
        let defender_player = 1 - attacker_player;

        let attacker = state.players[attacker_player]
            .characters
            .iter()
            .find(|c| c.card_id == attacker_id)?;
        let attacker_def = repo.get_by_id(attacker_id).ok()?;
        let attacker_power = attacker.effective_power(attacker_def.power);

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

    pub fn counter_from_hand_estimate(hand_count: u32) -> i32 {
        (hand_count as i32).min(5) * 1000
    }
}

use optcg_core::CardDefinition;
use tracing::info;

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
}
