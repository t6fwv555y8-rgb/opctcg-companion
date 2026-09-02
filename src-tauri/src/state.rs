use crate::dto::ObservationStatusDto;
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

    pub fn build_update_payload(
        &self,
        observation: Option<ObservationStatusDto>,
    ) -> StateUpdatePayload {
        let gs = self.game_state.read();
        let repo = self.repo();
        let combat_analysis = CombatMath::analyze_current_combat(&gs, &repo);
        let sync_confidence = observation
            .as_ref()
            .map(|o| match o.sync_state {
                crate::dto::SyncStateDto::Synced => 0.9,
                crate::dto::SyncStateDto::Partial => 0.6,
                crate::dto::SyncStateDto::Recovering => 0.5,
                crate::dto::SyncStateDto::Degraded => 0.35,
                crate::dto::SyncStateDto::Desynced => 0.2,
            })
            .unwrap_or(0.5);
        let eligibility = optcg_observation::AnalysisEligibility::evaluate(
            sync_confidence,
            gs.phase != optcg_core::Phase::Draw || gs.turn_number > 0,
            gs.player_one().life > 0,
            gs.combat.active,
            observation.is_some(),
        );
        let strategy = if eligibility.eligible {
            RulesEngine::recommend(&gs, &repo).ok().flatten()
        } else {
            None
        };
        let mut latency_ms = gs.connection.latency_ms;
        if let Some(ref obs) = observation {
            latency_ms = obs.latency.total_latency_ms;
        }

        let mut connection = ConnectionStatusDto::from_state(&gs);
        if let Some(ref obs) = observation {
            if let Some(ref src) = obs.active_source {
                connection = connection.with_source_label(Some(src));
            }
        }

        StateUpdatePayload {
            game_state: GameStateDto::from(&*gs),
            connection,
            combat_analysis,
            strategy,
            latency_ms,
            observation,
        }
    }
}
