use crate::confidence::ConfidenceConfig;
use crate::error::ObsResult;
use crate::session::{GameSession, SyncState};
use crate::types::ObservationEvent;
use chrono::Utc;
use optcg_core::{GameEvent, LastEventInfo, Normalizer, PlayerId};
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

        // Deck metadata is applied directly (not a GameEvent).
        if let ObservationEvent::StructuredRaw { raw, .. } = obs {
            if raw == "COMBAT_ACTIVE" {
                session.state.combat.active = true;
                session.state.phase = optcg_core::Phase::Combat;
                return Ok(ReconcileOutcome {
                    applied: true,
                    game_events: vec![],
                    corrected: false,
                    rejection_reason: None,
                    confidence,
                });
            }
            if let Some(page_state) = parse_page_state_raw(raw) {
                session.state.page_state = page_state;
                return Ok(ReconcileOutcome {
                    applied: true,
                    game_events: vec![],
                    corrected: false,
                    rejection_reason: None,
                    confidence,
                });
            }
            if let Some((player_idx, name)) = parse_player_name_raw(raw) {
                if let Some(p) = session.state.player_mut(player_idx) {
                    p.player_name = name;
                }
                return Ok(ReconcileOutcome {
                    applied: true,
                    game_events: vec![],
                    corrected: false,
                    rejection_reason: None,
                    confidence,
                });
            }
            if let Some((player_idx, name)) = parse_deck_name_raw(raw) {
                if let Some(p) = session.state.player_mut(player_idx) {
                    p.deck_name = name;
                }
                return Ok(ReconcileOutcome {
                    applied: true,
                    game_events: vec![],
                    corrected: false,
                    rejection_reason: None,
                    confidence,
                });
            }
            if let Some((player_idx, card_id)) = parse_note_card_raw(raw) {
                if let Some(p) = session.state.player_mut(player_idx) {
                    p.note_card(&card_id);
                }
                return Ok(ReconcileOutcome {
                    applied: true,
                    game_events: vec![],
                    corrected: false,
                    rejection_reason: None,
                    confidence,
                });
            }
        }

        if let ObservationEvent::TurnObserved { player, .. } = obs {
            session.state.active_player = player.index();
        }

        // Absolute hand-count updates from browser snapshots.
        if let ObservationEvent::HandCountObserved { player, count, .. } = obs {
            let idx = player.index() as usize;
            self.last_hand_count[idx] = Some(*count);
            if let Some(p) = session.state.player_mut(player.index()) {
                p.hand_count = *count as u32;
            }
            return Ok(ReconcileOutcome {
                applied: true,
                game_events: vec![],
                corrected: false,
                rejection_reason: None,
                confidence,
            });
        }

        let events = self.observation_to_game_events(session, obs)?;
        if events.is_empty() {
            // First absolute life observation (no delta yet) — still apply count.
            if let ObservationEvent::LifeObserved { player, count, .. } = obs {
                if let Some(p) = session.state.player_mut(player.index()) {
                    p.life = u32::from(*count);
                }
                return Ok(ReconcileOutcome {
                    applied: true,
                    game_events: vec![],
                    corrected: false,
                    rejection_reason: None,
                    confidence,
                });
            }
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
                attacker_player,
                target,
                observed_power,
                ..
            } => {
                if let Some(atk) = attacker_card_id {
                    let attacker_player = attacker_player.unwrap_or(PlayerId::Player1);
                    let tgt = target.clone().unwrap_or(optcg_core::AttackTarget::Leader {
                        player: attacker_player.opponent(),
                    });
                    vec![GameEvent::AttackDeclared {
                        attacker: atk.clone(),
                        attacker_player,
                        target: tgt,
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
                    Self::record_action(session, event);
                    return Ok(corrected);
                }
            }
        }

        Normalizer::apply_event(&mut session.state, event)?;
        Self::record_action(session, event);
        Ok(false)
    }

    /// Note what just happened in the action log.
    ///
    /// `Normalizer::apply_event` only mutates the board. The sequence,
    /// last-event, and log bookkeeping lives in `Normalizer::process`, which
    /// this path deliberately skips because observations arrive already parsed.
    /// Without this the action history stays empty for every observed source,
    /// leaving the HUD with no last event and the coach with no way to see what
    /// led to the current position.
    fn record_action(session: &mut GameSession, event: &GameEvent) {
        let state = &mut session.state;
        state.event_sequence += 1;
        let info = LastEventInfo {
            sequence: state.event_sequence,
            event_name: event.name().to_string(),
            summary: Normalizer::summarize(event),
            processed_at: Utc::now(),
        };
        state.push_log(format!("#{} {}", info.sequence, info.summary));
        state.last_event = Some(info);
        session.event_sequence = state.event_sequence;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ObservationSource;
    use optcg_core::Phase;

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
    fn applied_observations_build_up_an_action_log() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::Mock);

        for raw in [
            "PHASE_CHANGED|MAIN",
            "PHASE_CHANGED|END",
            "TURN_STARTED|PLAYER_1",
        ] {
            let obs = ObservationEvent::StructuredRaw {
                raw: raw.into(),
                source: ObservationSource::Mock,
                confidence: 1.0,
            };
            assert!(reconciler.reconcile(&mut session, &obs).unwrap().applied);
        }

        assert_eq!(
            session.state.event_log.len(),
            3,
            "every applied observation should be recorded: {:?}",
            session.state.event_log
        );
        assert!(
            session.state.event_log[0].starts_with("#1 PHASE_CHANGED"),
            "got {:?}",
            session.state.event_log[0]
        );
        assert_eq!(
            session.state.event_sequence, 3,
            "the sequence has to advance or every entry collides"
        );
        assert_eq!(session.event_sequence, session.state.event_sequence);

        let last = session
            .state
            .last_event
            .as_ref()
            .expect("the HUD reads the last event from here");
        assert_eq!(last.sequence, 3);
        assert_eq!(last.event_name, "TURN_STARTED");
    }

    #[test]
    fn a_rejected_observation_records_nothing() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::ScreenVision);
        let obs = ObservationEvent::PhaseObserved {
            phase: Phase::Main,
            confidence: 0.1,
        };

        assert!(!reconciler.reconcile(&mut session, &obs).unwrap().applied);
        assert!(
            session.state.event_log.is_empty(),
            "a guess the reconciler threw out must not appear as something that happened"
        );
        assert!(session.state.last_event.is_none());
        assert_eq!(session.state.event_sequence, 0);
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

    #[test]
    fn deck_name_structured_raw_applied() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::BrowserSimulator);
        let obs = ObservationEvent::StructuredRaw {
            raw: "DECK_NAME|PLAYER_1|Red Luffy Aggro".into(),
            source: ObservationSource::BrowserSimulator,
            confidence: 0.95,
        };
        let outcome = reconciler.reconcile(&mut session, &obs).unwrap();
        assert!(outcome.applied);
        assert_eq!(session.state.player_one().deck_name, "Red Luffy Aggro");
    }

    #[test]
    fn note_card_structured_raw_applied() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::BrowserSimulator);
        let obs = ObservationEvent::StructuredRaw {
            raw: "NOTE_CARD|PLAYER_2|ST01-002".into(),
            source: ObservationSource::BrowserSimulator,
            confidence: 0.9,
        };
        let outcome = reconciler.reconcile(&mut session, &obs).unwrap();
        assert!(outcome.applied);
        assert!(session
            .state
            .player_two()
            .known_cards
            .iter()
            .any(|c| c == "ST01-002"));
    }

    #[test]
    fn page_state_and_player_name_from_queue_snapshot() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::BrowserSimulator);
        let page = ObservationEvent::StructuredRaw {
            raw: "PAGE_STATE|queue".into(),
            source: ObservationSource::BrowserSimulator,
            confidence: 0.9,
        };
        let name = ObservationEvent::StructuredRaw {
            raw: "PLAYER_NAME|PLAYER_1|Jesus".into(),
            source: ObservationSource::BrowserSimulator,
            confidence: 0.9,
        };
        assert!(reconciler.reconcile(&mut session, &page).unwrap().applied);
        assert!(reconciler.reconcile(&mut session, &name).unwrap().applied);
        assert_eq!(session.state.page_state, "queue");
        assert_eq!(session.state.player_one().player_name, "Jesus");
    }

    #[test]
    fn opponent_attack_sets_combat_on_your_leader() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::BrowserSimulator);
        let obs = ObservationEvent::AttackObserved {
            attacker: None,
            attacker_card_id: Some("ST01-012".into()),
            attacker_player: Some(PlayerId::Player2),
            target: Some(optcg_core::AttackTarget::Leader {
                player: PlayerId::Player1,
            }),
            observed_power: Some(6000),
            confidence: 0.9,
        };
        assert!(reconciler.reconcile(&mut session, &obs).unwrap().applied);
        assert!(session.state.combat.active);
        assert_eq!(session.state.combat.attacker_id.as_deref(), Some("ST01-012"));
        assert_eq!(session.state.combat.attacker_player, Some(1));
        assert_eq!(session.state.combat.target_player, Some(0));
        assert!(session.state.combat.target_is_leader);
    }

    #[test]
    fn combat_active_without_ids_still_marks_a_fight() {
        let mut reconciler = ObservationReconciler::default();
        let mut session = GameSession::new(ObservationSource::BrowserSimulator);
        let obs = ObservationEvent::StructuredRaw {
            raw: "COMBAT_ACTIVE".into(),
            source: ObservationSource::BrowserSimulator,
            confidence: 0.9,
        };
        assert!(reconciler.reconcile(&mut session, &obs).unwrap().applied);
        assert!(session.state.combat.active);
        assert_eq!(session.state.phase, Phase::Combat);
    }
}

fn parse_page_state_raw(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw.splitn(2, '|').collect();
    if parts.len() != 2 || parts[0] != "PAGE_STATE" {
        return None;
    }
    let state = parts[1].trim().to_ascii_lowercase();
    matches!(state.as_str(), "queue" | "lobby" | "match").then_some(state)
}

fn parse_player_name_raw(raw: &str) -> Option<(u8, String)> {
    let parts: Vec<&str> = raw.splitn(3, '|').collect();
    if parts.len() != 3 || parts[0] != "PLAYER_NAME" {
        return None;
    }
    let idx = match parts[1] {
        "PLAYER_1" | "P1" | "0" => 0u8,
        "PLAYER_2" | "P2" | "1" => 1u8,
        _ => return None,
    };
    let name = parts[2].trim();
    if name.is_empty() {
        return None;
    }
    Some((idx, name.to_string()))
}

fn parse_deck_name_raw(raw: &str) -> Option<(u8, String)> {
    let parts: Vec<&str> = raw.splitn(3, '|').collect();
    if parts.len() != 3 || parts[0] != "DECK_NAME" {
        return None;
    }
    let idx = match parts[1] {
        "PLAYER_1" | "P1" | "0" => 0u8,
        "PLAYER_2" | "P2" | "1" => 1u8,
        _ => return None,
    };
    let name = parts[2].trim();
    if name.is_empty() {
        return None;
    }
    Some((idx, name.to_string()))
}

fn parse_note_card_raw(raw: &str) -> Option<(u8, String)> {
    let parts: Vec<&str> = raw.splitn(3, '|').collect();
    if parts.len() != 3 || parts[0] != "NOTE_CARD" {
        return None;
    }
    let idx = match parts[1] {
        "PLAYER_1" | "P1" | "0" => 0u8,
        "PLAYER_2" | "P2" | "1" => 1u8,
        _ => return None,
    };
    let card_id = parts[2].trim();
    if card_id.is_empty() {
        return None;
    }
    Some((idx, card_id.to_string()))
}
