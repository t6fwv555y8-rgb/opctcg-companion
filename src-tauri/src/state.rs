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
            beam: BeamSearch::new(BeamSearchConfig {
                beam_width: 4,
                max_depth: 3,
            }),
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
        let phase_coach = RulesEngine::phase_coach(&gs);

        let sync_confidence = observation
            .as_ref()
            .map(|o| match o.sync_state {
                crate::dto::SyncStateDto::Synced => 0.9,
                crate::dto::SyncStateDto::Partial => 0.7,
                crate::dto::SyncStateDto::Recovering => 0.55,
                crate::dto::SyncStateDto::Degraded => 0.4,
                crate::dto::SyncStateDto::Desynced => 0.2,
            })
            .unwrap_or(0.55);

        let source_connected = observation
            .as_ref()
            .map(|o| {
                !matches!(
                    o.hud_state,
                    crate::dto::HudOperatingState::Lost | crate::dto::HudOperatingState::Searching
                )
            })
            .unwrap_or(true);

        let eligibility = optcg_observation::AnalysisEligibility::evaluate(
            sync_confidence,
            true,
            gs.player_one().life > 0 || gs.player_two().life > 0,
            gs.combat.active,
            source_connected,
        );

        // Always outline options for the current step when we have any usable state.
        // Full "best move" confidence still respects eligibility.
        let mut options = RulesEngine::rank_actions(&gs, &repo).unwrap_or_default();
        if options.len() > 8 {
            options.truncate(8);
        }

        // Prefer beam-search top line when eligible — richer multi-step sequencing.
        let strategy = if eligibility.eligible || eligibility.mode != optcg_observation::AnalysisMode::Paused
        {
            if let Ok(beam) = self.beam.recommend(&gs, &repo) {
                if let Some(top) = beam.first() {
                    Some(optcg_rules::StrategyRecommendation {
                        action: optcg_rules::Action {
                            action_type: top.action.action_type.clone(),
                            actor: gs.active_player,
                            card_id: top.action.card_id.clone(),
                            target: top.action.target_id.clone(),
                            target_player: top.action.target_player,
                            cost: top.action.cost,
                            description: if top.sequence.len() > 1 {
                                format!("Line: {}", top.sequence.join(" → "))
                            } else {
                                top.action.description.clone()
                            },
                        },
                        score: top.score,
                        confidence: if eligibility.eligible { 0.75 } else { 0.45 },
                        reasoning: if top.sequence.len() > 1 {
                            format!(
                                "Suggested sequence for this phase: {}. {}",
                                top.sequence.join(" → "),
                                phase_coach
                            )
                        } else {
                            phase_coach.clone()
                        },
                    })
                } else {
                    options.first().cloned()
                }
            } else {
                options.first().cloned()
            }
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
            options,
            phase_coach,
            latency_ms,
            observation,
        }
    }
}
