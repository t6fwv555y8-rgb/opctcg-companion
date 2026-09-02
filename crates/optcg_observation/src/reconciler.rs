use crate::confidence::ConfidenceConfig;
use crate::error::{ObsResult, ObservationError};
use crate::session::{GameSession, SyncState};
use crate::types::{ObservationEvent, ObservationSource};
use optcg_core::{GameEvent, Normalizer, Phase, PlayerId};
use tracing::{debug, info, warn};

/// Outcome of reconciling one observation into engine events.
#[derive(Debug, Clone)]
pub struct ReconcileOutcome {
    pub applied: bool,
    pub game_events: Vec<GameEvent>,
    pub corrected: bool,
    pub rejection_reason: Option<String>,
    pub confidence: f32,
}

/// Converts uncertain observations into validated engine events.
pub struct ObservationReconciler {
    config: ConfidenceConfig,
    last_life: [Option<u8>; 2],
    last_hand_count: [Option<usize>; 2],
}

impl Default for ObservationReconciler {
    fn default() -> Self {
        Self::new(ConfidenceConfig::default())
    }
}

impl ObservationReconciler {
    pub fn new(config: ConfidenceConfig) -> Self {
        Self {
            config,
            last_life: [None, None],
            last_hand_count: [None, None],
        }
    }

    pub fn sync_state(&self, session: &GameSession) -> SyncState {
        session.sync_state()
    }

    pub fn reconcile(
        &mut self,
        session: &mut GameSession,
        obs: &ObservationEvent,
    ) -> ObsResult<ReconcileOutcome> {
        let confidence = obs.confidence();
        if confidence < self.config.min_apply {
            return Ok(ReconcileOutcome {
                applied: false,
                game_events: vec![],
                corrected: false,
                rejection_reason: Some(format!("confidence {confidence} below min_apply")),
                confidence,
            });
        }

        session.observation_sequence += 1;

        if let ObservationEvent::TurnObserved { player, .. } = obs {
            session.state.active_player = player.index();
        }

        let events = self.observation_to_game_events(session, obs)?;
        if events.is_empty() {
            return Ok(ReconcileOutcome {
                applied: false,
                game_events: vec![],
                corrected: false,
                rejection_reason: Some("no mappable game events".into()),
                confidence,
            });
        }

        let mut corrected = false;
        let mut applied_any = false;

        for event in &events {
            match self.apply_with_correction(session, event, confidence) {
                Ok(was_correction) => {
                    applied_any = true;
                    corrected |= was_correction;
                }
                Err(e) => {
                    warn!(error = %e, "failed to apply reconciled event");
                }
            }
        }

        session.confidence = session.confidence * 0.9 + confidence * 0.1;

        debug!(
            target: "optcg::reconcile",
            applied = applied_any,
            corrected,
            confidence,
            "observation reconciled"
        );

        Ok(ReconcileOutcome {
            applied: applied_any,
            game_events: events,
            corrected,
            rejection_reason: None,
            confidence,
        })
    }

    fn observation_to_game_events(
        &mut self,
        session: &GameSession,
        obs: &ObservationEvent,
    ) -> ObsResult<Vec<GameEvent>> {
        Ok(match obs {
            ObservationEvent::StructuredRaw { raw, .. } => {
                vec![Normalizer::parse_event(raw)?]
            }
            ObservationEvent::GameDetected { source, .. } => {
                if session.source != *source {
                    vec![GameEvent::GameStarted]
                } else {
                    vec![]
                }
            }
            ObservationEvent::PhaseObserved { phase, .. } => {
                vec![GameEvent::PhaseChanged { phase: *phase }]
            }
            ObservationEvent::TurnObserved { .. } => vec![],
            ObservationEvent::LifeObserved { player, count, .. } => {
                let idx = player.index() as usize;
                // Partial state: never infer zero from missing observations
                if *count == 0 && self.last_life[idx].is_some() && self.last_life[idx] != Some(0) {
                    return Ok(vec![]);
                }
                let prev = self.last_life[idx];
                let delta = if let Some(prev_count) = prev {
                    *count as i8 - prev_count as i8
                } else {
                    0
                };
                if prev != Some(*count) {
                    if let Some(from) = prev {
                        info!(
                            target: "optcg::reconcile",
                            "[RECONCILE] life {:?} {} → {} source={:?} confidence={:.2}",
                            player,
                            from,
                            count,
                            session.source,
                            obs.confidence()
                        );
                    }
                    self.last_life[idx] = Some(*count);
                }
                if delta != 0 {
                    vec![GameEvent::LifeChanged {
                        player: *player,
                        delta,
                    }]
                } else if prev.is_none() {
                    // First observation — set absolute via synthetic delta from default life
                    vec![]
                } else {
                    vec![]
                }
            }
            ObservationEvent::DonObserved {
                player,
                active,
                attached,
                ..
            } => {
                if *attached > 0 {
                    vec![GameEvent::DonAttached {
                        player: *player,
                        target: "LEADER".into(),
                        amount: *attached,
                    }]
                } else if *active > 0 {
                    vec![]
                } else {
                    vec![]
                }
            }
            ObservationEvent::HandCountObserved { player, count, .. } => {
                let idx = player.index() as usize;
                if self.last_hand_count[idx] != Some(*count) {
                    self.last_hand_count[idx] = Some(*count);
                }
                vec![]
            }
            ObservationEvent::CardObserved {
                card_id,
                owner,
                zone,
                ..
            } => {
                if let Some(cid) = card_id {
                    vec![GameEvent::CardPlayed {
                        player: *owner,
                        card_id: cid.clone(),
                        zone: Some(format!("{:?}", zone).to_lowercase()),
                    }]
                } else {
                    vec![]
                }
            }
            ObservationEvent::CardMoved { card_id, to, .. } => {
                if let Some(cid) = card_id {
                    if *to == optcg_core::Zone::Trash {
                        vec![GameEvent::CardRemovedFromBoard {
                            player: PlayerId::Player1,
                            card_instance: cid.clone(),
                        }]
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            ObservationEvent::AttackObserved {
                attacker_card_id,
                target,
                observed_power,
                ..
            } => {
                if let (Some(atk), Some(tgt)) = (attacker_card_id, target) {
                    vec![GameEvent::AttackDeclared {
                        attacker: atk.clone(),
                        attacker_player: PlayerId::Player1,
                        target: tgt.clone(),
                        power: observed_power.unwrap_or(0),
                    }]
                } else {
                    vec![]
                }
            }
        })
    }

    fn apply_with_correction(
        &mut self,
        session: &mut GameSession,
        event: &GameEvent,
        confidence: f32,
    ) -> ObsResult<bool> {
        // State correction: high-confidence life observations can override
        if let GameEvent::LifeChanged { player, delta } = event {
            if confidence >= self.config.correction_threshold {
                if let Some(p) = session.state.player_mut(player.index()) {
                    let corrected = *delta > 0 && p.life != p.life.saturating_add(*delta as u32);
                    Normalizer::apply_event(&mut session.state, event)?;
                    session.event_sequence = session.state.event_sequence;
                    return Ok(corrected);
                }
            }
        }

        Normalizer::apply_event(&mut session.state, event)?;
        session.event_sequence = session.state.event_sequence;
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ObservationSource;

    #[test]
    fn structured_raw_reconciles_to_phase() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::Mock);
        let obs = ObservationEvent::StructuredRaw {
            raw: "PHASE_CHANGED|MAIN".into(),
            source: ObservationSource::Mock,
            confidence: 1.0,
        };
        let outcome = reconciler.reconcile(&mut session, &obs).unwrap();
        assert!(outcome.applied);
        assert_eq!(session.state.phase, Phase::Main);
    }

    #[test]
    fn low_confidence_rejected() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::ScreenVision);
        let obs = ObservationEvent::PhaseObserved {
            phase: Phase::Main,
            confidence: 0.1,
        };
        let outcome = reconciler.reconcile(&mut session, &obs).unwrap();
        assert!(!outcome.applied);
    }

    #[test]
    fn life_correction_applied() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::BrowserSimulator);
        reconciler
            .reconcile(
                &mut session,
                &ObservationEvent::LifeObserved {
                    player: PlayerId::Player1,
                    count: 5,
                    confidence: 0.99,
                },
            )
            .unwrap();
        let outcome = reconciler
            .reconcile(
                &mut session,
                &ObservationEvent::LifeObserved {
                    player: PlayerId::Player1,
                    count: 4,
                    confidence: 0.99,
                },
            )
            .unwrap();
        assert!(outcome.applied);
        assert_eq!(session.state.player_one().life, 4);
    }
}
