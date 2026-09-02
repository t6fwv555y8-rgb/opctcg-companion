use crate::bridge_protocol::BrowserGameSnapshot;
use crate::confidence::ConfidenceConfig;
use crate::types::{ObservationEvent, ObservationSource};
use optcg_core::{Phase, PlayerId, Zone};

/// Diff two browser snapshots into observation events.
pub struct SnapshotDiffer {
    config: ConfidenceConfig,
    last: Option<BrowserGameSnapshot>,
}

impl SnapshotDiffer {
    pub fn new(config: ConfidenceConfig) -> Self {
        Self { config, last: None }
    }

    pub fn diff(&mut self, snapshot: &BrowserGameSnapshot) -> Vec<ObservationEvent> {
        let confidence = ConfidenceConfig::for_source(ObservationSource::BrowserSimulator);
        let mut events = Vec::new();

        if let Some(phase) = &snapshot.phase {
            events.push(ObservationEvent::PhaseObserved {
                phase: Phase::from_str_loose(phase),
                confidence,
            });
        }

        if let Some(turn) = snapshot.turn {
            let player = if turn % 2 == 1 {
                PlayerId::Player1
            } else {
                PlayerId::Player2
            };
            events.push(ObservationEvent::TurnObserved { player, confidence });
        }

        if let Some(side) = &snapshot.self_player {
            if let Some(life) = side.life {
                events.push(ObservationEvent::LifeObserved {
                    player: PlayerId::Player1,
                    count: life,
                    confidence,
                });
            }
            if let Some(hand) = side.hand_count {
                events.push(ObservationEvent::HandCountObserved {
                    player: PlayerId::Player1,
                    count: hand as usize,
                    confidence,
                });
            }
            if let (Some(active), Some(rested)) = (side.active_don, side.rested_don) {
                events.push(ObservationEvent::DonObserved {
                    player: PlayerId::Player1,
                    active,
                    rested,
                    attached: 0,
                    confidence,
                });
            }
            for card in &side.board {
                if let Some(id) = &card.card_id {
                    events.push(ObservationEvent::CardObserved {
                        card_id: Some(id.clone()),
                        owner: PlayerId::Player1,
                        zone: Zone::Character,
                        position: None,
                        confidence,
                    });
                }
            }
        }

        if let Some(opp) = &snapshot.opponent {
            if let Some(life) = opp.life {
                if self
                    .last
                    .as_ref()
                    .and_then(|p| p.opponent.as_ref())
                    .and_then(|o| o.life)
                    != Some(life)
                {
                    events.push(ObservationEvent::LifeObserved {
                        player: PlayerId::Player2,
                        count: life,
                        confidence,
                    });
                }
            }
        }

        if let Some(combat) = &snapshot.combat {
            if combat.attacker.is_some() || combat.target.is_some() {
                events.push(ObservationEvent::AttackObserved {
                    attacker: None,
                    attacker_card_id: combat.attacker.as_ref().and_then(|c| c.card_id.clone()),
                    target: combat
                        .target
                        .as_ref()
                        .map(|t| optcg_core::AttackTarget::Leader {
                            player: PlayerId::Player2,
                        }),
                    observed_power: combat.displayed_power,
                    confidence,
                });
            }
        }

        self.last = Some(snapshot.clone());
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge_protocol::{BrowserPlayerSnapshot, ObservedCard};

    #[test]
    fn diff_detects_life_change() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        let s1 = BrowserGameSnapshot {
            timestamp: 1,
            opponent: Some(BrowserPlayerSnapshot {
                life: Some(5),
                ..Default::default()
            }),
            ..Default::default()
        };
        differ.diff(&s1);
        let s2 = BrowserGameSnapshot {
            timestamp: 2,
            opponent: Some(BrowserPlayerSnapshot {
                life: Some(4),
                ..Default::default()
            }),
            ..Default::default()
        };
        let events = differ.diff(&s2);
        assert!(events
            .iter()
            .any(|e| matches!(e, ObservationEvent::LifeObserved { .. })));
    }
}
