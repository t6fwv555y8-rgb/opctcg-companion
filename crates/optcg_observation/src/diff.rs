use crate::bridge_protocol::BrowserGameSnapshot;
use crate::confidence::ConfidenceConfig;
use crate::types::{ObservationEvent, ObservationSource};
use optcg_core::{Phase, PlayerId, Zone};

/// Diff two browser snapshots into observation events.
pub struct SnapshotDiffer {
    last: Option<BrowserGameSnapshot>,
}

impl SnapshotDiffer {
    pub fn new(_config: ConfidenceConfig) -> Self {
        Self { last: None }
    }

    pub fn diff(&mut self, snapshot: &BrowserGameSnapshot) -> Vec<ObservationEvent> {
        let confidence = ConfidenceConfig::for_source(ObservationSource::BrowserSimulator);
        let mut events = Vec::new();
        let prev = self.last.as_ref();

        if let Some(phase) = &snapshot.phase {
            let changed = prev.and_then(|p| p.phase.as_ref()) != Some(phase);
            if changed || prev.is_none() {
                events.push(ObservationEvent::PhaseObserved {
                    phase: Phase::from_str_loose(phase),
                    confidence,
                });
            }
        }

        // Trust the page's own "whose turn" wording. Turn parity is only a
        // fallback: it is wrong whenever the opponent goes first.
        let stated = snapshot.active_player.as_deref().and_then(|a| match a {
            "self" => Some(PlayerId::Player1),
            "opponent" => Some(PlayerId::Player2),
            _ => None,
        });
        let prev_stated = prev.and_then(|p| p.active_player.as_deref());
        if let Some(player) = stated {
            if prev_stated != snapshot.active_player.as_deref() || prev.is_none() {
                events.push(ObservationEvent::TurnObserved { player, confidence });
            }
        } else if let Some(turn) = snapshot.turn {
            let changed = prev.and_then(|p| p.turn) != Some(turn);
            if changed || prev.is_none() {
                let player = if turn % 2 == 1 {
                    PlayerId::Player1
                } else {
                    PlayerId::Player2
                };
                events.push(ObservationEvent::TurnObserved { player, confidence });
            }
        }

        if let Some(side) = &snapshot.self_player {
            events.extend(diff_player(
                side,
                PlayerId::Player1,
                prev.and_then(|p| p.self_player.as_ref()),
                confidence,
            ));
        }

        if let Some(opp) = &snapshot.opponent {
            events.extend(diff_player(
                opp,
                PlayerId::Player2,
                prev.and_then(|p| p.opponent.as_ref()),
                confidence,
            ));
        }

        match (&snapshot.combat, prev.and_then(|p| p.combat.as_ref())) {
            (Some(combat), prev_combat) => {
                let has_signal = combat.attacker.is_some()
                    || combat.target.is_some()
                    || combat.displayed_power.is_some();
                let key = |c: &crate::bridge_protocol::BrowserCombatSnapshot| {
                    (
                        c.attacker.as_ref().and_then(|a| a.card_id.clone()),
                        c.attacker_owner.clone(),
                        c.target.as_ref().and_then(|t| t.card_id.clone()),
                        c.target_owner.clone(),
                        c.displayed_power,
                    )
                };
                let changed = prev_combat.map(key) != Some(key(combat));
                if has_signal && (changed || prev.is_none()) {
                    let owner = |o: &Option<String>| match o.as_deref() {
                        Some("self") => Some(PlayerId::Player1),
                        Some("opponent") => Some(PlayerId::Player2),
                        _ => None,
                    };
                    let attacker_player = owner(&combat.attacker_owner);
                    // The defender is whoever is not attacking. Assume the
                    // opponent when the page did not say who attacked, which
                    // is the only case the old code handled.
                    let defender = owner(&combat.target_owner).unwrap_or_else(|| {
                        match attacker_player {
                            Some(PlayerId::Player2) => PlayerId::Player1,
                            _ => PlayerId::Player2,
                        }
                    });
                    // An attack with no visible target is most often aimed at
                    // the Leader; say so with a lower confidence.
                    let target_seen = combat.target.is_some();
                    let target = if combat.target_is_leader.unwrap_or(!target_seen) {
                        Some(optcg_core::AttackTarget::Leader { player: defender })
                    } else {
                        combat
                            .target
                            .as_ref()
                            .and_then(|t| t.card_id.clone())
                            .map(|card_id| optcg_core::AttackTarget::Character {
                                player: defender,
                                card_id,
                            })
                    };
                    events.push(ObservationEvent::AttackObserved {
                        attacker: None,
                        attacker_player,
                        attacker_card_id: combat.attacker.as_ref().and_then(|c| c.card_id.clone()),
                        target,
                        observed_power: combat.displayed_power,
                        confidence: if target_seen { confidence } else { confidence * 0.7 },
                    });
                }
            }
            (None, Some(_)) => {
                // The battle UI went away: the attack is over.
                events.push(ObservationEvent::StructuredRaw {
                    raw: "COMBAT_CLEARED".into(),
                    source: ObservationSource::BrowserSimulator,
                    confidence,
                });
            }
            (None, None) => {}
        }

        // Report the end of a game once, and again if the result shows up later.
        if snapshot.game_over == Some(true) {
            let was_over = prev.is_some_and(|p| p.game_over == Some(true));
            let result_changed = prev.map(|p| p.result.clone()) != Some(snapshot.result.clone());
            if !was_over || result_changed {
                let word = match snapshot.result.as_deref() {
                    Some("win") => "WIN",
                    Some("loss") => "LOSS",
                    _ => "UNKNOWN",
                };
                events.push(ObservationEvent::StructuredRaw {
                    raw: format!("GAME_OVER|{word}"),
                    source: ObservationSource::BrowserSimulator,
                    confidence,
                });
            }
        }

        if snapshot
            .diagnostics
            .as_ref()
            .is_some_and(|d| d.game_detected == Some(true))
            && prev.is_none()
        {
            events.push(ObservationEvent::GameDetected {
                source: ObservationSource::BrowserSimulator,
                confidence,
            });
        }

        self.last = Some(snapshot.clone());
        events
    }
}

fn diff_player(
    side: &crate::bridge_protocol::BrowserPlayerSnapshot,
    player: PlayerId,
    prev: Option<&crate::bridge_protocol::BrowserPlayerSnapshot>,
    confidence: f32,
) -> Vec<ObservationEvent> {
    let mut events = Vec::new();

    if let Some(life) = side.life {
        if prev.and_then(|p| p.life) != Some(life) {
            events.push(ObservationEvent::LifeObserved {
                player,
                count: life,
                confidence,
            });
        }
    }

    if let Some(deck_name) = &side.deck_name {
        let prev_name = prev.and_then(|p| p.deck_name.as_ref());
        if prev_name != Some(deck_name) && !deck_name.is_empty() {
            let label = match player {
                PlayerId::Player1 => "PLAYER_1",
                PlayerId::Player2 => "PLAYER_2",
            };
            events.push(ObservationEvent::StructuredRaw {
                raw: format!("DECK_NAME|{label}|{deck_name}"),
                source: ObservationSource::BrowserSimulator,
                confidence,
            });
        }
    }

    if let Some(hand) = side.hand_count {
        if prev.and_then(|p| p.hand_count) != Some(hand) {
            events.push(ObservationEvent::HandCountObserved {
                player,
                count: hand as usize,
                confidence,
            });
        }
    }

    if let Some(hand) = &side.hand {
        if prev.and_then(|p| p.hand.as_ref()) != Some(hand) {
            let label = match player {
                PlayerId::Player1 => "PLAYER_1",
                PlayerId::Player2 => "PLAYER_2",
            };
            events.push(ObservationEvent::StructuredRaw {
                raw: format!("HAND|{label}|{}", hand.join(",")),
                source: ObservationSource::BrowserSimulator,
                confidence,
            });
        }
    }

    if let (Some(active), Some(rested)) = (side.active_don, side.rested_don) {
        let prev_don = prev.map(|p| (p.active_don, p.rested_don));
        if prev_don != Some((Some(active), Some(rested))) {
            events.push(ObservationEvent::DonObserved {
                player,
                active,
                rested,
                attached: 0,
                confidence,
            });
        }
    }

    let prev_board: Vec<_> = prev.map(|p| p.board.clone()).unwrap_or_default();
    for card in &side.board {
        let key = card.instance_key.as_deref().or(card.card_id.as_deref());
        let existed = prev_board
            .iter()
            .any(|c| c.instance_key.as_deref().or(c.card_id.as_deref()) == key);
        if !existed {
            if let Some(id) = &card.card_id {
                let zone = zone_from_instance_key(card.instance_key.as_deref());
                events.push(ObservationEvent::CardObserved {
                    card_id: Some(id.clone()),
                    owner: player,
                    zone,
                    position: None,
                    confidence,
                });
            }
        }
    }

    if let Some(leader_id) = &side.leader_id {
        let prev_leader = prev.and_then(|p| p.leader_id.as_ref());
        if prev_leader != Some(leader_id) {
            events.push(ObservationEvent::CardObserved {
                card_id: Some(leader_id.clone()),
                owner: player,
                zone: Zone::Leader,
                position: None,
                confidence,
            });
        }
    }

    // Accumulate newly seen known cards (self hand / trash) without polluting board zones.
    let prev_known = prev.map(|p| p.known_cards.clone()).unwrap_or_default();
    let label = match player {
        PlayerId::Player1 => "PLAYER_1",
        PlayerId::Player2 => "PLAYER_2",
    };
    for id in &side.known_cards {
        if !prev_known.iter().any(|k| k == id) {
            let on_board = side
                .board
                .iter()
                .any(|c| c.card_id.as_deref() == Some(id.as_str()));
            if !on_board && side.leader_id.as_deref() != Some(id.as_str()) {
                events.push(ObservationEvent::StructuredRaw {
                    raw: format!("NOTE_CARD|{label}|{id}"),
                    source: ObservationSource::BrowserSimulator,
                    confidence: confidence * 0.9,
                });
            }
        }
    }

    for prev_card in &prev_board {
        let key = prev_card
            .instance_key
            .as_deref()
            .or(prev_card.card_id.as_deref());
        let still = side
            .board
            .iter()
            .any(|c| c.instance_key.as_deref().or(c.card_id.as_deref()) == key);
        if !still {
            if let Some(id) = &prev_card.card_id {
                events.push(ObservationEvent::CardMoved {
                    instance_id: None,
                    card_id: Some(id.clone()),
                    from: Zone::Character,
                    to: Zone::Trash,
                    confidence,
                });
            }
        }
    }

    events
}

fn zone_from_instance_key(key: Option<&str>) -> Zone {
    let Some(key) = key else {
        return Zone::Character;
    };
    // Format: player:zone:slot:cardId:iid
    let zone = key.split(':').nth(1).unwrap_or("character");
    match zone {
        "leader" => Zone::Leader,
        "stage" => Zone::Stage,
        "hand" => Zone::Hand,
        "trash" => Zone::Trash,
        "life" => Zone::Life,
        _ => Zone::Character,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge_protocol::{BrowserPlayerSnapshot, ObservedCard};

    #[test]
    fn hand_contents_are_reported_once_and_again_when_they_change() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        let snap = |hand: &[&str]| BrowserGameSnapshot {
            timestamp: 1,
            self_player: Some(BrowserPlayerSnapshot {
                hand: Some(hand.iter().map(|c| c.to_string()).collect()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let raws = |events: Vec<ObservationEvent>| {
            events
                .into_iter()
                .filter_map(|e| match e {
                    ObservationEvent::StructuredRaw { raw, .. } if raw.starts_with("HAND|") => {
                        Some(raw)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            raws(differ.diff(&snap(&["ST01-002", "ST01-002"]))),
            vec!["HAND|PLAYER_1|ST01-002,ST01-002"]
        );
        assert!(raws(differ.diff(&snap(&["ST01-002", "ST01-002"]))).is_empty());
        assert_eq!(raws(differ.diff(&snap(&["ST01-002"]))).len(), 1);
    }

    fn combat_snapshot(
        attacker_owner: &str,
        target_owner: &str,
        target_is_leader: bool,
    ) -> BrowserGameSnapshot {
        use crate::bridge_protocol::BrowserCombatSnapshot;
        let card = |id: &str| ObservedCard {
            card_id: Some(id.into()),
            ..Default::default()
        };
        BrowserGameSnapshot {
            timestamp: 1,
            combat: Some(BrowserCombatSnapshot {
                attacker: Some(card("ST01-002")),
                target: Some(card("ST01-001")),
                displayed_power: Some(6000),
                attacker_owner: Some(attacker_owner.into()),
                target_owner: Some(target_owner.into()),
                target_is_leader: Some(target_is_leader),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn an_opponent_attack_on_your_leader_is_not_read_as_your_own() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        let events = differ.diff(&combat_snapshot("opponent", "self", true));
        let attack = events
            .iter()
            .find_map(|e| match e {
                ObservationEvent::AttackObserved {
                    attacker_player,
                    target,
                    ..
                } => Some((*attacker_player, target.clone())),
                _ => None,
            })
            .expect("attack event");
        assert_eq!(attack.0, Some(PlayerId::Player2));
        assert!(matches!(
            attack.1,
            Some(optcg_core::AttackTarget::Leader { player: PlayerId::Player1 })
        ));
    }

    #[test]
    fn combat_is_cleared_when_the_battle_ui_goes_away() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        differ.diff(&combat_snapshot("opponent", "self", true));
        let events = differ.diff(&BrowserGameSnapshot {
            timestamp: 2,
            ..Default::default()
        });
        assert!(events.iter().any(|e| matches!(
            e,
            ObservationEvent::StructuredRaw { raw, .. } if raw == "COMBAT_CLEARED"
        )));
    }

    #[test]
    fn game_over_is_reported_once_and_again_when_the_result_appears() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        let snap = |over: bool, result: Option<&str>| BrowserGameSnapshot {
            timestamp: 1,
            game_over: Some(over),
            result: result.map(str::to_string),
            ..Default::default()
        };
        let raws = |events: Vec<ObservationEvent>| -> Vec<String> {
            events
                .into_iter()
                .filter_map(|e| match e {
                    ObservationEvent::StructuredRaw { raw, .. } if raw.starts_with("GAME_OVER") => Some(raw),
                    _ => None,
                })
                .collect()
        };
        assert!(raws(differ.diff(&snap(false, None))).is_empty());
        assert_eq!(raws(differ.diff(&snap(true, None))), vec!["GAME_OVER|UNKNOWN"]);
        assert!(raws(differ.diff(&snap(true, None))).is_empty(), "no repeat");
        assert_eq!(raws(differ.diff(&snap(true, Some("win")))), vec!["GAME_OVER|WIN"]);
    }

    #[test]
    fn stated_active_player_beats_turn_parity() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        // Turn 1 would be Player1 by parity, but the page says it is the opponent's.
        let events = differ.diff(&BrowserGameSnapshot {
            timestamp: 1,
            turn: Some(1),
            active_player: Some("opponent".into()),
            ..Default::default()
        });
        assert!(events.iter().any(|e| matches!(
            e,
            ObservationEvent::TurnObserved { player: PlayerId::Player2, .. }
        )));
    }

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

    #[test]
    fn same_life_not_re_emitted() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        let snap = BrowserGameSnapshot {
            timestamp: 1,
            self_player: Some(BrowserPlayerSnapshot {
                life: Some(5),
                ..Default::default()
            }),
            ..Default::default()
        };
        differ.diff(&snap);
        let events = differ.diff(&snap);
        assert!(!events
            .iter()
            .any(|e| matches!(e, ObservationEvent::LifeObserved { .. })));
    }

    #[test]
    fn detects_board_card_appears_and_leaves() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        let s1 = BrowserGameSnapshot {
            timestamp: 1,
            self_player: Some(BrowserPlayerSnapshot {
                board: vec![ObservedCard {
                    card_id: Some("OP01-001".into()),
                    instance_key: Some("0:character:0:OP01-001:i1".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let appear = differ.diff(&s1);
        assert!(appear
            .iter()
            .any(|e| matches!(e, ObservationEvent::CardObserved { .. })));

        let s2 = BrowserGameSnapshot {
            timestamp: 2,
            self_player: Some(BrowserPlayerSnapshot {
                board: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let leave = differ.diff(&s2);
        assert!(leave
            .iter()
            .any(|e| matches!(e, ObservationEvent::CardMoved { .. })));
    }

    #[test]
    fn duplicate_card_ids_tracked_by_instance_key() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        let s1 = BrowserGameSnapshot {
            timestamp: 1,
            self_player: Some(BrowserPlayerSnapshot {
                board: vec![
                    ObservedCard {
                        card_id: Some("OP01-001".into()),
                        instance_key: Some("0:character:0:OP01-001:i1".into()),
                        ..Default::default()
                    },
                    ObservedCard {
                        card_id: Some("OP01-001".into()),
                        instance_key: Some("0:character:1:OP01-001:i2".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let events = differ.diff(&s1);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, ObservationEvent::CardObserved { .. }))
                .count(),
            2
        );
    }
}
