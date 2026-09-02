use crate::combat_math::CombatMath;
use crate::error::RulesResult;
use optcg_core::{CardDefinition, GameState, Phase, Zone};
use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};

/// Classification of a legal player action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    AttachDon,
    PlayCharacter,
    PlayStage,
    PlayEvent,
    AttackLeader,
    AttackCharacter,
    Pass,
    ActivateBlocker,
    EndPhase,
    EndTurn,
}

/// High-level candidate action for strategy scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub action_type: ActionType,
    pub actor: u8,
    pub card_id: Option<String>,
    pub target: Option<String>,
    pub target_player: Option<u8>,
    pub cost: u32,
    pub description: String,
}

/// Legacy legal action with priority (retained for beam/MCTS).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalAction {
    pub action_type: ActionType,
    pub card_id: Option<String>,
    pub target_id: Option<String>,
    pub target_player: Option<u8>,
    pub cost: u32,
    pub description: String,
    pub priority: i32,
}

/// Deterministic strategy recommendation with human-readable reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRecommendation {
    pub action: Action,
    pub score: f64,
    pub confidence: f64,
    pub reasoning: String,
}

/// Deterministic rules parser scanning state for legal player actions.
pub struct RulesEngine;

impl RulesEngine {
    pub fn generate_actions(state: &GameState, repo: &CardRepository) -> RulesResult<Vec<Action>> {
        Ok(Self::legal_actions(state, repo)?
            .into_iter()
            .map(|la| Action {
                action_type: la.action_type,
                actor: la.target_player.unwrap_or(state.active_player),
                card_id: la.card_id,
                target: la.target_id,
                target_player: la.target_player,
                cost: la.cost,
                description: la.description,
            })
            .collect())
    }

    pub fn recommend(
        state: &GameState,
        repo: &CardRepository,
    ) -> RulesResult<Option<StrategyRecommendation>> {
        let actions = Self::generate_actions(state, repo)?;
        if actions.is_empty() {
            return Ok(None);
        }

        let mut best: Option<StrategyRecommendation> = None;
        for action in actions {
            let rec = Self::score_action(state, repo, &action);
            if best.as_ref().map(|b| rec.score > b.score).unwrap_or(true) {
                best = Some(rec);
            }
        }

        if let Some(ref top) = best {
            info!(
                target: "optcg::strategy",
                action = %top.action.description,
                score = top.score,
                "strategy recommendation"
            );
        }

        Ok(best)
    }

    fn score_action(
        state: &GameState,
        repo: &CardRepository,
        action: &Action,
    ) -> StrategyRecommendation {
        let mut score = 0.0f64;
        let mut reasons = Vec::new();

        match action.action_type {
            ActionType::AttackLeader => {
                score += 8.0;
                reasons.push("Attack leader first to create life pressure before committing DON.");
                if state.combat.active {
                    score += 2.0;
                }
            }
            ActionType::AttackCharacter => {
                score += 5.0;
                reasons.push("Remove an opponent character to improve board advantage.");
            }
            ActionType::AttachDon => {
                score += 4.0;
                reasons.push("Attach DON efficiently to prepare a stronger attack next.");
            }
            ActionType::PlayCharacter => {
                score += 6.0;
                reasons.push("Develop board presence to increase future pressure.");
                if action.cost <= state.players[state.active_player as usize].don_active {
                    score += 1.5;
                    reasons.push("Play is affordable with active DON.");
                } else {
                    score -= 2.0;
                    reasons.push("Expensive play — consider DON allocation first.");
                }
            }
            ActionType::ActivateBlocker => {
                score += 9.0;
                reasons.push("Block incoming attack to preserve life total.");
            }
            ActionType::Pass | ActionType::EndPhase | ActionType::EndTurn => {
                score += 1.0;
                reasons.push("Pass if no profitable line is available.");
            }
            _ => {
                score += 2.0;
            }
        }

        let opponent = 1 - state.active_player as usize;
        if state.players[state.active_player as usize].life < state.players[opponent].life {
            score += 1.0;
            reasons.push("You are behind on life — prioritize pressure.");
        }

        if let Some(combat) = CombatMath::analyze_current_combat(state, repo) {
            if combat.lethal_to_leader {
                score += 3.0;
                reasons.push("Current combat line is lethal — high value attack.");
            }
        }

        let confidence = (score / 10.0).clamp(0.1, 0.95);
        StrategyRecommendation {
            action: action.clone(),
            score,
            confidence,
            reasoning: reasons.join(" "),
        }
    }

    pub fn legal_actions(
        state: &GameState,
        repo: &CardRepository,
    ) -> RulesResult<Vec<LegalAction>> {
        let mut actions = Vec::new();
        let player_idx = state.active_player;
        let player = &state.players[player_idx as usize];
        let opponent_idx = 1 - player_idx as usize;
        let opponent = &state.players[opponent_idx];

        if state.phase == Phase::Don {
            if player.don_active > 0 {
                for character in &player.characters {
                    if character.zone == Zone::Character {
                        actions.push(LegalAction {
                            action_type: ActionType::AttachDon,
                            card_id: Some(character.card_id.clone()),
                            target_id: None,
                            target_player: Some(player_idx),
                            cost: 1,
                            description: format!("Attach DON!! to {}", character.card_id),
                            priority: 10,
                        });
                    }
                }
                actions.push(LegalAction {
                    action_type: ActionType::AttachDon,
                    card_id: Some(player.leader.card_id.clone()),
                    target_id: None,
                    target_player: Some(player_idx),
                    cost: 1,
                    description: format!("Attach DON!! to Leader {}", player.leader.card_id),
                    priority: 8,
                });
            }
        }

        if state.phase == Phase::Main || state.phase == Phase::Don {
            for card in &player.hand {
                if let Ok(def) = repo.get_by_id(&card.card_id) {
                    if player.don_active + player.don_rested >= def.cost {
                        let action_type = match def.card_type {
                            optcg_core::CardType::Character => ActionType::PlayCharacter,
                            optcg_core::CardType::Stage => ActionType::PlayStage,
                            optcg_core::CardType::Event => ActionType::PlayEvent,
                            _ => continue,
                        };
                        actions.push(LegalAction {
                            action_type,
                            card_id: Some(def.card_id.clone()),
                            target_id: None,
                            target_player: Some(player_idx),
                            cost: def.cost,
                            description: format!("Play {} (cost {})", def.name, def.cost),
                            priority: 20 + def.power as i32,
                        });
                    }
                }
            }
        }

        if state.phase == Phase::Main || state.phase == Phase::Combat {
            for character in &player.characters {
                if !character.tapped && !character.rested && character.zone == Zone::Character {
                    if let Ok(def) = repo.get_by_id(&character.card_id) {
                        if def.keywords.rush || state.turn_number > 1 {
                            actions.push(LegalAction {
                                action_type: ActionType::AttackLeader,
                                card_id: Some(character.card_id.clone()),
                                target_id: Some("leader".into()),
                                target_player: Some(opponent_idx as u8),
                                cost: 0,
                                description: format!("{} attacks opponent Leader", def.name),
                                priority: 50 + def.power as i32,
                            });
                        }

                        for opp_char in &opponent.characters {
                            actions.push(LegalAction {
                                action_type: ActionType::AttackCharacter,
                                card_id: Some(character.card_id.clone()),
                                target_id: Some(opp_char.card_id.clone()),
                                target_player: Some(opponent_idx as u8),
                                cost: 0,
                                description: format!("{} attacks {}", def.name, opp_char.card_id),
                                priority: 40 + def.power as i32,
                            });
                        }
                    }
                }
            }
        }

        if state.combat.blocker_offered {
            if let Some(blocker) = player.find_blocker() {
                actions.push(LegalAction {
                    action_type: ActionType::ActivateBlocker,
                    card_id: Some(blocker.card_id.clone()),
                    target_id: state.combat.attacker_id.clone(),
                    target_player: Some(player_idx),
                    cost: 0,
                    description: format!("Block with {}", blocker.card_id),
                    priority: 100,
                });
            }
        }

        actions.push(LegalAction {
            action_type: ActionType::EndPhase,
            card_id: None,
            target_id: None,
            target_player: None,
            cost: 0,
            description: "Advance phase".into(),
            priority: 1,
        });

        if state.phase == Phase::End {
            actions.push(LegalAction {
                action_type: ActionType::EndTurn,
                card_id: None,
                target_id: None,
                target_player: None,
                cost: 0,
                description: "End turn".into(),
                priority: 5,
            });
        }

        actions.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(actions)
    }

    pub fn card_power(def: &CardDefinition, attached_don: u32) -> i32 {
        def.power as i32 + attached_don as i32 * 1000
    }
}

use tracing::info;

#[cfg(test)]
mod tests {
    use super::*;
    use optcg_core::GameState;

    #[test]
    fn generate_actions_includes_pass_phase() {
        let state = GameState::new();
        let db = optcg_database::Database::open_in_memory().unwrap();
        let _ = optcg_database::AssetParser::seed_defaults(&db);
        let repo = optcg_database::CardRepository::new(&db);
        let actions = RulesEngine::generate_actions(&state, &repo).unwrap();
        assert!(!actions.is_empty());
    }

    #[test]
    fn recommend_returns_scored_action() {
        let state = GameState::new();
        let db = optcg_database::Database::open_in_memory().unwrap();
        let _ = optcg_database::AssetParser::seed_defaults(&db);
        let repo = optcg_database::CardRepository::new(&db);
        let rec = RulesEngine::recommend(&state, &repo).unwrap();
        assert!(rec.is_some());
        assert!(!rec.unwrap().reasoning.is_empty());
    }
}
