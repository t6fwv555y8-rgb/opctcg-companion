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
    ActivateBlocker,
    EndPhase,
    EndTurn,
}

/// A single legal action with metadata for UI and search layers.
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

/// Deterministic rules parser scanning state for legal player actions.
pub struct RulesEngine;

impl RulesEngine {
    pub fn legal_actions(state: &GameState, repo: &CardRepository) -> RulesResult<Vec<LegalAction>> {
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
                    card_id: Some(player.leader_id.clone()),
                    target_id: None,
                    target_player: Some(player_idx),
                    cost: 1,
                    description: format!("Attach DON!! to Leader {}", player.leader_id),
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
                if !character.tapped && character.zone == Zone::Character {
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
                                description: format!(
                                    "{} attacks {}",
                                    def.name, opp_char.card_id
                                ),
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
