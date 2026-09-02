use crate::dto::{ConnectionStatusDto, GameStateDto, OverlaySettings, StateUpdatePayload};
use optcg_database::Database;
use optcg_rules::{BeamSearch, BeamSearchConfig, CombatMath, MctsConfig, MctsEngine, RulesEngine};
use parking_lot::RwLock;
use std::sync::Arc;

pub struct AppState {
    pub database: Database,
    pub game_state: Arc<RwLock<optcg_core::GameState>>,
    pub beam: BeamSearch,
    pub mcts: MctsEngine,
    pub overlay: RwLock<OverlaySettings>,
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
            overlay: RwLock::new(OverlaySettings {
                click_through: false,
                opacity: 0.92,
            }),
        }
    }

    pub fn repo(&self) -> optcg_database::CardRepository<'_> {
        optcg_database::CardRepository::new(&self.database)
    }

    pub fn build_update_payload(&self) -> StateUpdatePayload {
        let gs = self.game_state.read();
        let repo = self.repo();
        let combat_analysis = CombatMath::analyze_current_combat(&gs, &repo);
        let strategy = RulesEngine::recommend(&gs, &repo).ok().flatten();
        let latency_ms = gs.connection.latency_ms;

        StateUpdatePayload {
            game_state: GameStateDto::from(&*gs),
            connection: ConnectionStatusDto::from_state(&gs),
            combat_analysis,
            strategy,
            latency_ms,
        }
    }
}
