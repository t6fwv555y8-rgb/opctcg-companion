use optcg_database::Database;
use optcg_rules::{BeamSearch, BeamSearchConfig, MctsConfig, MctsEngine};
use parking_lot::RwLock;
use std::sync::Arc;

pub struct AppState {
    pub database: Database,
    pub game_state: Arc<RwLock<optcg_core::GameState>>,
    pub beam: BeamSearch,
    pub mcts: MctsEngine,
}

impl AppState {
    pub fn new(database: Database, game_state: Arc<RwLock<optcg_core::GameState>>) -> Self {
        Self {
            database,
            game_state,
            beam: BeamSearch::new(BeamSearchConfig::default()),
            mcts: MctsEngine::new(MctsConfig {
                iterations: 100,
                ..Default::default()
            }),
        }
    }

    pub fn repo(&self) -> optcg_database::CardRepository<'_> {
        optcg_database::CardRepository::new(&self.database)
    }
}
