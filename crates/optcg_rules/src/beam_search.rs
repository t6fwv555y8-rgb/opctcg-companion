use crate::engine::{ActionType, LegalAction, RulesEngine};
use crate::error::RulesResult;
use optcg_core::{GameState, Normalizer, RawEvent};
use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Configuration for beam search sequencing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeamSearchConfig {
    pub beam_width: usize,
    pub max_depth: usize,
}

impl Default for BeamSearchConfig {
    fn default() -> Self {
        Self {
            beam_width: 5,
            max_depth: 4,
        }
    }
}

/// An action with heuristic score for instant sequencing recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredAction {
    pub action: LegalAction,
    pub score: f64,
    pub sequence: Vec<String>,
}

/// Instant sequencing scorer using bounded beam search.
pub struct BeamSearch {
    config: BeamSearchConfig,
}

impl BeamSearch {
    pub fn new(config: BeamSearchConfig) -> Self {
        Self { config }
    }

    pub fn recommend(
        &self,
        state: &GameState,
        repo: &CardRepository,
    ) -> RulesResult<Vec<ScoredAction>> {
        let legal = RulesEngine::legal_actions(state, repo)?;
        if legal.is_empty() {
            return Ok(Vec::new());
        }

        let mut beam: Vec<(GameState, Vec<LegalAction>, f64)> = legal
            .iter()
            .map(|action| {
                let mut sim = state.clone();
                Self::apply_action(&mut sim, action);
                let score = Self::evaluate(&sim, repo);
                (sim, vec![action.clone()], score)
            })
            .collect();

        beam.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        beam.truncate(self.config.beam_width);

        for _depth in 1..self.config.max_depth {
            let mut next_beam = Vec::new();
            for (sim_state, sequence, _score) in &beam {
                let next_actions = RulesEngine::legal_actions(sim_state, repo)?;
                for action in next_actions.iter().take(self.config.beam_width) {
                    let mut new_sim = sim_state.clone();
                    Self::apply_action(&mut new_sim, action);
                    let new_score = Self::evaluate(&new_sim, repo);
                    let mut new_seq = sequence.clone();
                    new_seq.push(action.clone());
                    next_beam.push((new_sim, new_seq, new_score));
                }
            }
            if next_beam.is_empty() {
                break;
            }
            next_beam.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
            next_beam.truncate(self.config.beam_width);
            beam = next_beam;
        }

        let results = beam
            .into_iter()
            .map(|(_state, sequence, score)| {
                let action = sequence.last().cloned().unwrap_or(LegalAction {
                    action_type: ActionType::EndPhase,
                    card_id: None,
                    target_id: None,
                    target_player: None,
                    cost: 0,
                    description: "No action".into(),
                    priority: 0,
                });
                ScoredAction {
                    action,
                    score,
                    sequence: sequence
                        .iter()
                        .map(|a| a.description.clone())
                        .collect(),
                }
            })
            .collect();

        Ok(results)
    }

    fn evaluate(state: &GameState, repo: &CardRepository) -> f64 {
        let active = state.active_player as usize;
        let opponent = 1 - active;
        let mut score = 0.0f64;

        score += state.players[active].life as f64 * 1000.0;
        score -= state.players[opponent].life as f64 * 1200.0;

        for character in &state.players[active].characters {
            if let Ok(def) = repo.get_by_id(&character.card_id) {
                score += character.effective_power(def.power) as f64 * 0.5;
                score += character.attached_don as f64 * 200.0;
            }
        }

        score += state.players[active].don_active as f64 * 150.0;
        score += state.players[active].hand_count as f64 * 80.0;

        if state.combat.active && state.combat.attacker_player == Some(state.active_player) {
            score += 500.0;
        }

        score
    }

    fn apply_action(state: &mut GameState, action: &LegalAction) {
        let event = match action.action_type {
            ActionType::AttachDon => RawEvent {
                event_type: "DON_ATTACHED".into(),
                payload: json!({
                    "player": action.target_player.unwrap_or(state.active_player),
                    "card_id": action.card_id,
                    "amount": 1
                }),
            },
            ActionType::PlayCharacter | ActionType::PlayStage | ActionType::PlayEvent => {
                RawEvent {
                    event_type: "CARD_PLAYED".into(),
                    payload: json!({
                        "player": action.target_player.unwrap_or(state.active_player),
                        "card_id": action.card_id,
                        "zone": "character"
                    }),
                }
            }
            ActionType::AttackLeader | ActionType::AttackCharacter => RawEvent {
                event_type: "COMBAT_DECLARED".into(),
                payload: json!({
                    "attacker": action.card_id,
                    "target": action.target_id,
                    "target_player": action.target_player
                }),
            },
            ActionType::ActivateBlocker => RawEvent {
                event_type: "BLOCKER_OFFERED".into(),
                payload: json!({
                    "player": action.target_player,
                    "blocker_id": action.card_id
                }),
            },
            ActionType::EndTurn => RawEvent {
                event_type: "TURN_END".into(),
                payload: json!({
                    "next_player": 1 - state.active_player
                }),
            },
            ActionType::EndPhase => RawEvent {
                event_type: "PHASE_CHANGED".into(),
                payload: json!({
                    "phase": "End",
                    "active_player": state.active_player
                }),
            },
        };
        let _ = Normalizer::apply_event(state, &event);
    }
}
