use optcg_core::{CardDefinition, GameState};
use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};

/// Result of a combat interaction analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatAnalysis {
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

        let (defender_power, blocker_available) = if target_is_leader {
            let leader = &state.players[defender_player];
            (
                leader.leader_power as i32,
                leader.find_blocker().is_some(),
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
            )
        };

        let differential = attacker_power - defender_power;
        let required_counter = differential.max(0);
        let base_counter = if target_is_leader {
            0
        } else {
            defender_id
                .and_then(|id| repo.get_by_id(id).ok())
                .map(|d| d.keywords.counter_value())
                .unwrap_or(0)
        };

        let survives_without = differential <= 0;
        let survives_with_base = differential <= base_counter;
        let lethal_to_leader = target_is_leader && differential > 0;

        let recommended_block = !survives_without
            && blocker_available
            && differential < 8000;

        Some(CombatAnalysis {
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

    pub fn optimal_counter_play(
        available_counters: &[CardDefinition],
        required: i32,
    ) -> Option<&CardDefinition> {
        available_counters
            .iter()
            .filter(|c| c.counter >= required)
            .min_by_key(|c| c.counter)
            .or_else(|| {
                available_counters
                    .iter()
                    .max_by_key(|c| c.counter)
            })
    }
}
