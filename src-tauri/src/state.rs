use crate::dto::ObservationStatusDto;
use crate::dto::{
    ConnectionStatusDto, DeckInfoDto, GameStateDto, KnownCardDto, OverlaySettings,
    StateUpdatePayload,
};
use optcg_database::Database;
use optcg_rules::{
    BeamSearch, BeamSearchConfig, CombatMath, DeckProfile, DeckStrategyBrief, DeckStrategyCoach,
    MctsConfig, MctsEngine, RulesEngine,
};
use parking_lot::RwLock;
use std::sync::Arc;

pub struct AppState {
    pub database: Database,
    pub game_state: Arc<RwLock<optcg_core::GameState>>,
    pub beam: BeamSearch,
    pub mcts: MctsEngine,
    pub overlay: RwLock<OverlaySettings>,
    /// Cached deck strategy brief; refreshed on demand or when matchup changes.
    pub deck_strategy: RwLock<Option<DeckStrategyBrief>>,
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
            deck_strategy: RwLock::new(None),
        }
    }

    pub fn repo(&self) -> optcg_database::CardRepository<'_> {
        optcg_database::CardRepository::new(&self.database)
    }

    fn profile_from_deck(deck: &DeckInfoDto) -> DeckProfile {
        DeckProfile {
            name: deck.name.clone(),
            leader_id: deck.leader_id.clone(),
            leader_name: deck.leader_name.clone(),
            leader_color: deck.leader_color.clone(),
            known_card_ids: deck.known_cards.iter().map(|c| c.card_id.clone()).collect(),
            known_card_names: deck.known_cards.iter().map(|c| c.name.clone()).collect(),
        }
    }

    /// Rebuild detailed deck-vs-deck strategy for the current matchup.
    pub fn refresh_deck_strategy(&self) -> DeckStrategyBrief {
        let gs = self.game_state.read();
        let your_deck = self.deck_info_for(gs.player_one());
        let opponent_deck = self.deck_info_for(gs.player_two());
        let brief = DeckStrategyCoach::brief(
            &gs,
            &Self::profile_from_deck(&your_deck),
            &Self::profile_from_deck(&opponent_deck),
        );
        *self.deck_strategy.write() = Some(brief.clone());
        brief
    }

    fn ensure_deck_strategy(
        &self,
        your_deck: &DeckInfoDto,
        opponent_deck: &DeckInfoDto,
        gs: &optcg_core::GameState,
    ) -> DeckStrategyBrief {
        let matchup = format!("{} vs {}", your_deck.name, opponent_deck.name);
        let mut cache = self.deck_strategy.write();
        let needs_refresh = match cache.as_ref() {
            None => true,
            Some(existing) => existing.matchup != matchup,
        };
        if needs_refresh {
            let brief = DeckStrategyCoach::brief(
                gs,
                &Self::profile_from_deck(your_deck),
                &Self::profile_from_deck(opponent_deck),
            );
            *cache = Some(brief.clone());
            brief
        } else {
            cache.as_ref().unwrap().clone()
        }
    }

    fn deck_info_for(&self, player: &optcg_core::PlayerState) -> DeckInfoDto {
        let repo = self.repo();
        let leader_id = player.leader.card_id.clone();
        let (leader_name, leader_color) = match repo.get_by_id(&leader_id) {
            Ok(def) => (def.name, def.color),
            Err(_) => {
                if leader_id.is_empty() {
                    ("Unknown leader".into(), String::new())
                } else {
                    (leader_id.clone(), String::new())
                }
            }
        };

        let mut known_cards: Vec<KnownCardDto> = player
            .known_cards
            .iter()
            .filter(|id| !id.is_empty())
            .map(|id| match repo.get_by_id(id) {
                Ok(def) => KnownCardDto {
                    card_id: id.clone(),
                    name: def.name,
                    card_type: format!("{:?}", def.card_type).to_lowercase(),
                    color: def.color,
                },
                Err(_) => KnownCardDto {
                    card_id: id.clone(),
                    name: id.clone(),
                    card_type: "unknown".into(),
                    color: String::new(),
                },
            })
            .collect();
        known_cards.sort_by(|a, b| a.name.cmp(&b.name));

        let name = if !player.deck_name.trim().is_empty() {
            player.deck_name.clone()
        } else if !leader_color.is_empty() && leader_name != "Unknown leader" {
            format!("{leader_color} {leader_name}")
        } else if !leader_id.is_empty() {
            format!("Deck · {leader_id}")
        } else {
            "Deck unknown".into()
        };

        DeckInfoDto {
            name,
            leader_id,
            leader_name,
            leader_color,
            known_cards,
        }
    }

    pub fn build_update_payload(
        &self,
        observation: Option<ObservationStatusDto>,
    ) -> StateUpdatePayload {
        let gs = self.game_state.read();
        let repo = self.repo();
        let combat_analysis = CombatMath::analyze_current_combat(&gs, &repo);
        let your_deck = self.deck_info_for(gs.player_one());
        let opponent_deck = self.deck_info_for(gs.player_two());
        let deck_strategy = self.ensure_deck_strategy(&your_deck, &opponent_deck, &gs);
        let mut phase_coach = RulesEngine::phase_coach(&gs);
        // Make coaching deck-specific when we know the matchup.
        if your_deck.leader_id != "" || opponent_deck.leader_id != "" {
            let you = if your_deck.name != "Deck unknown" {
                your_deck.name.as_str()
            } else {
                "your deck"
            };
            let opp = if opponent_deck.name != "Deck unknown" {
                opponent_deck.name.as_str()
            } else {
                "opponent"
            };
            phase_coach = format!("{phase_coach} ({you} vs {opp})");
        }

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
                                "Suggested sequence for this phase: {}. {} | {}",
                                top.sequence.join(" → "),
                                phase_coach,
                                deck_strategy.vs_opponent
                            )
                        } else {
                            format!("{} | {}", phase_coach, deck_strategy.your_plan)
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
            deck_strategy: Some(deck_strategy),
            your_deck,
            opponent_deck,
            latency_ms,
            observation,
        }
    }
}
