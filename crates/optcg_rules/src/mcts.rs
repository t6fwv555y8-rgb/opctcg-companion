use crate::engine::{LegalAction, RulesEngine};
use crate::error::RulesResult;
use crate::sim::simulate_action;
use optcg_core::GameState;
use optcg_database::CardRepository;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MCTS configuration parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MctsConfig {
    pub iterations: u32,
    pub exploration: f64,
    pub max_rollout_depth: u32,
}

impl Default for MctsConfig {
    fn default() -> Self {
        Self {
            iterations: 200,
            exploration: 1.41,
            max_rollout_depth: 12,
        }
    }
}

/// Result of MCTS analysis with win-rate estimates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MctsResult {
    pub best_action: LegalAction,
    pub win_rate: f64,
    pub visits: u32,
    pub alternatives: Vec<(LegalAction, f64)>,
}

struct MctsNode {
    action: Option<LegalAction>,
    visits: u32,
    total_value: f64,
    children: HashMap<String, MctsNode>,
    untried: Vec<LegalAction>,
}

impl MctsNode {
    fn new(action: Option<LegalAction>, untried: Vec<LegalAction>) -> Self {
        Self {
            action,
            visits: 0,
            total_value: 0.0,
            children: HashMap::new(),
            untried,
        }
    }

    fn ucb1(&self, parent_visits: u32, exploration: f64) -> f64 {
        if self.visits == 0 {
            return f64::INFINITY;
        }
        let exploitation = self.total_value / self.visits as f64;
        let exploration_term =
            exploration * ((parent_visits as f64).ln() / self.visits as f64).sqrt();
        exploitation + exploration_term
    }
}

/// Monte Carlo Tree Search with opponent hand determinization.
pub struct MctsEngine {
    config: MctsConfig,
}

impl MctsEngine {
    pub fn new(config: MctsConfig) -> Self {
        Self { config }
    }

    pub fn search(&self, state: &GameState, repo: &CardRepository) -> RulesResult<MctsResult> {
        let root_actions = RulesEngine::legal_actions(state, repo)?;
        if root_actions.is_empty() {
            return Err(crate::error::RulesError::InvalidAction(
                "no legal actions".into(),
            ));
        }

        let mut root = MctsNode::new(None, root_actions.clone());

        for _ in 0..self.config.iterations {
            let mut sim_state = state.clone();
            Self::determinize_opponent_hand(&mut sim_state);

            let mut node_ptr = &mut root;
            let mut path: Vec<String> = Vec::new();
            let mut depth = 0u32;

            while depth < self.config.max_rollout_depth {
                if !node_ptr.untried.is_empty() {
                    let action = node_ptr.untried.pop().unwrap();
                    let key = Self::action_key(&action);
                    simulate_action(&mut sim_state, &action);
                    let child_actions =
                        RulesEngine::legal_actions(&sim_state, repo).unwrap_or_default();
                    let child = MctsNode::new(Some(action.clone()), child_actions);
                    node_ptr.children.insert(key.clone(), child);
                    path.push(key.clone());
                    node_ptr = node_ptr.children.get_mut(&key).unwrap();
                    depth += 1;
                    break;
                }

                if node_ptr.children.is_empty() {
                    break;
                }

                let best_key = node_ptr
                    .children
                    .iter()
                    .max_by(|(_, a), (_, b)| {
                        a.ucb1(node_ptr.visits, self.config.exploration)
                            .partial_cmp(&b.ucb1(node_ptr.visits, self.config.exploration))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(k, _)| k.clone())
                    .unwrap();

                if let Some(action) = node_ptr
                    .children
                    .get(&best_key)
                    .and_then(|n| n.action.clone())
                {
                    simulate_action(&mut sim_state, &action);
                }
                path.push(best_key.clone());
                node_ptr = node_ptr.children.get_mut(&best_key).unwrap();
                depth += 1;
            }

            let value =
                Self::rollout_value(&mut sim_state, repo, self.config.max_rollout_depth - depth);
            Self::backpropagate(&mut root, &path, value);
        }

        let mut alternatives: Vec<(LegalAction, f64)> = root
            .children
            .values()
            .filter_map(|child| {
                child.action.as_ref().map(|a| {
                    let rate = if child.visits > 0 {
                        child.total_value / child.visits as f64
                    } else {
                        0.0
                    };
                    (a.clone(), rate)
                })
            })
            .collect();

        alternatives.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let (best_action, win_rate, visits) = alternatives
            .first()
            .cloned()
            .map(|(a, r)| {
                (
                    a,
                    r,
                    root.children.values().map(|c| c.visits).max().unwrap_or(0),
                )
            })
            .unwrap_or((root_actions[0].clone(), 0.5, 0));

        Ok(MctsResult {
            best_action,
            win_rate,
            visits,
            alternatives,
        })
    }

    fn determinize_opponent_hand(state: &mut GameState) {
        let mut rng = rand::thread_rng();
        let opponent_idx = (1 - state.active_player as usize) as u8;
        if let Some(opponent) = state.player_mut(opponent_idx) {
            let estimated = opponent.hand_count;
            opponent.hand.clear();
            let stub_ids = ["ST01-002", "ST01-003", "ST01-010", "ST01-012"];
            for _i in 0..estimated.min(5) {
                let idx = rng.gen_range(0..stub_ids.len());
                opponent.hand.push(optcg_core::CardInstance::new(
                    stub_ids[idx],
                    opponent_idx,
                    optcg_core::Zone::Hand,
                ));
            }
        }
    }

    fn action_key(action: &LegalAction) -> String {
        format!(
            "{:?}:{:?}:{:?}",
            action.action_type, action.card_id, action.target_id
        )
    }

    fn rollout_value(state: &mut GameState, repo: &CardRepository, remaining_depth: u32) -> f64 {
        let mut rng = rand::thread_rng();
        let mut depth = 0;
        while depth < remaining_depth {
            let actions = RulesEngine::legal_actions(state, repo).unwrap_or_default();
            if actions.is_empty() {
                break;
            }
            let idx = rng.gen_range(0..actions.len());
            simulate_action(state, &actions[idx]);
            depth += 1;
        }

        let active = state.active_player as usize;
        let opponent = 1 - active;
        let mut value = 0.0;
        value += state.players[active].life as f64 * 2.0;
        value -= state.players[opponent].life as f64 * 2.5;
        value += state.players[active].characters.len() as f64 * 0.3;
        value -= state.players[opponent].characters.len() as f64 * 0.25;

        if state.players[opponent].life == 0 {
            value += 10.0;
        }
        if state.players[active].life == 0 {
            value -= 10.0;
        }

        1.0 / (1.0 + (-value).exp())
    }

    fn backpropagate(root: &mut MctsNode, path: &[String], value: f64) {
        Self::backpropagate_node(root, path, value);
    }

    fn backpropagate_node(node: &mut MctsNode, path: &[String], value: f64) {
        node.visits += 1;
        node.total_value += value;
        if let Some((first, rest)) = path.split_first() {
            if let Some(child) = node.children.get_mut(first) {
                Self::backpropagate_node(child, rest, value);
            }
        }
    }
}
