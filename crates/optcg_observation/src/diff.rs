use crate::bridge_protocol::{
    BrowserCombatSnapshot, BrowserGameSnapshot, BrowserPlayerSnapshot,
};
use crate::confidence::ConfidenceConfig;
use crate::types::{ObservationEvent, ObservationSource};
use optcg_core::{AttackTarget, Phase, PlayerId, Zone};

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

        if let Some(page_state) = snapshot
            .page_state
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let changed = prev.and_then(|p| p.page_state.as_deref()) != Some(page_state);
            if changed || prev.is_none() {
                events.push(ObservationEvent::StructuredRaw {
                    raw: format!("PAGE_STATE|{page_state}"),
                    source: ObservationSource::BrowserSimulator,
                    confidence,
                });
            }
        }

        if let Some(phase) = &snapshot.phase {
            let changed = prev.and_then(|p| p.phase.as_ref()) != Some(phase);
            if changed || prev.is_none() {
                events.push(ObservationEvent::PhaseObserved {
                    phase: Phase::from_str_loose(phase),
                    confidence,
                });
            }
        }

        if let Some(turn) = snapshot.turn {
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

        let prev_combat = prev.and_then(|p| p.combat.as_ref());
        let now_fighting = snapshot.combat.as_ref().is_some_and(combat_signal);
        let was_fighting = prev_combat.is_some_and(combat_signal);

        if let Some(combat) = &snapshot.combat {
            if combat_signal(combat) {
                let changed = prev_combat.map(combat_fingerprint) != Some(combat_fingerprint(combat));
                if changed || prev.is_none() {
                    if let Some(attacker_card_id) =
                        combat.attacker.as_ref().and_then(|c| c.card_id.clone())
                    {
                        let attacker_player = combat_side_player(
                            combat.attacker_player.as_deref(),
                            snapshot,
                            combat.attacker.as_ref().and_then(|c| c.card_id.as_deref()),
                        );
                        events.push(ObservationEvent::AttackObserved {
                            attacker: None,
                            attacker_card_id: Some(attacker_card_id),
                            attacker_player,
                            target: Some(combat_target(snapshot, combat, attacker_player)),
                            // Displayed power is the printed total, not a modifier.
                            // AttackDeclared treats power as a bonus, so omit it here.
                            observed_power: None,
                            confidence,
                        });
                    } else {
                        events.push(ObservationEvent::StructuredRaw {
                            raw: "COMBAT_ACTIVE".into(),
                            source: ObservationSource::BrowserSimulator,
                            confidence,
                        });
                    }
                    if combat.blocker_offered == Some(true) {
                        let blocker = combat.blocker_id.as_deref().unwrap_or("blocker");
                        let defender = combat_side_player(
                            combat.target_player.as_deref(),
                            snapshot,
                            combat.target.as_ref().and_then(|c| c.card_id.as_deref()),
                        )
                        .unwrap_or(PlayerId::Player1);
                        let label = match defender {
                            PlayerId::Player1 => "PLAYER_1",
                            PlayerId::Player2 => "PLAYER_2",
                        };
                        events.push(ObservationEvent::StructuredRaw {
                            raw: format!("BLOCKER_OFFERED|{label}|{blocker}"),
                            source: ObservationSource::BrowserSimulator,
                            confidence,
                        });
                    }
                }
            }
        }

        if was_fighting && !now_fighting {
            events.push(ObservationEvent::StructuredRaw {
                raw: "COMBAT_RESOLVED".into(),
                source: ObservationSource::BrowserSimulator,
                confidence,
            });
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

    if let Some(player_name) = &side.player_name {
        let prev_name = prev.and_then(|p| p.player_name.as_ref());
        if prev_name != Some(player_name) && !player_name.trim().is_empty() {
            let label = match player {
                PlayerId::Player1 => "PLAYER_1",
                PlayerId::Player2 => "PLAYER_2",
            };
            events.push(ObservationEvent::StructuredRaw {
                raw: format!("PLAYER_NAME|{label}|{player_name}"),
                source: ObservationSource::BrowserSimulator,
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

fn combat_signal(combat: &BrowserCombatSnapshot) -> bool {
    combat.active == Some(true)
        || combat.attacker.as_ref().and_then(|c| c.card_id.as_ref()).is_some()
        || combat.target.as_ref().and_then(|c| c.card_id.as_ref()).is_some()
        || combat.displayed_power.is_some()
        || combat.blocker_offered == Some(true)
}

fn combat_fingerprint(combat: &BrowserCombatSnapshot) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        combat.attacker.as_ref().and_then(|c| c.card_id.as_ref()),
        combat.target.as_ref().and_then(|c| c.card_id.as_ref()),
        combat.displayed_power,
        combat.attacker_player,
        combat.target_player,
        combat.target_is_leader,
        combat.blocker_offered
    )
}

fn owns_card(player: &BrowserPlayerSnapshot, card_id: &str) -> bool {
    player.leader_id.as_deref() == Some(card_id)
        || player
            .board
            .iter()
            .any(|c| c.card_id.as_deref() == Some(card_id))
}

fn combat_side_player(
    side: Option<&str>,
    snapshot: &BrowserGameSnapshot,
    card_id: Option<&str>,
) -> Option<PlayerId> {
    match side.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("self" | "you" | "0" | "player_1" | "player1" | "p1") => Some(PlayerId::Player1),
        Some("opponent" | "them" | "1" | "player_2" | "player2" | "p2") => Some(PlayerId::Player2),
        _ => {
            let id = card_id?;
            if snapshot
                .self_player
                .as_ref()
                .is_some_and(|p| owns_card(p, id))
            {
                Some(PlayerId::Player1)
            } else if snapshot.opponent.as_ref().is_some_and(|p| owns_card(p, id)) {
                Some(PlayerId::Player2)
            } else {
                None
            }
        }
    }
}

fn combat_target(
    snapshot: &BrowserGameSnapshot,
    combat: &BrowserCombatSnapshot,
    attacker_player: Option<PlayerId>,
) -> AttackTarget {
    let target_player = combat_side_player(
        combat.target_player.as_deref(),
        snapshot,
        combat.target.as_ref().and_then(|c| c.card_id.as_deref()),
    )
    .or_else(|| attacker_player.map(PlayerId::opponent))
    .unwrap_or(PlayerId::Player2);

    let target_id = combat
        .target
        .as_ref()
        .and_then(|c| c.card_id.as_deref())
        .unwrap_or("leader");
    let target_is_leader = combat.target_is_leader.unwrap_or_else(|| {
        target_id.eq_ignore_ascii_case("leader")
            || snapshot
                .self_player
                .as_ref()
                .and_then(|p| p.leader_id.as_deref())
                == Some(target_id)
            || snapshot
                .opponent
                .as_ref()
                .and_then(|p| p.leader_id.as_deref())
                == Some(target_id)
    });

    if target_is_leader {
        AttackTarget::Leader {
            player: target_player,
        }
    } else {
        AttackTarget::Character {
            player: target_player,
            card_id: target_id.to_string(),
        }
    }
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
    use crate::bridge_protocol::{BrowserCombatSnapshot, BrowserPlayerSnapshot, ObservedCard};

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

    #[test]
    fn queue_snapshot_emits_page_state_and_player_name() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        let snap = BrowserGameSnapshot {
            timestamp: 1,
            page_state: Some("queue".into()),
            self_player: Some(BrowserPlayerSnapshot {
                player_name: Some("Jesus".into()),
                leader_id: Some("OP01-001".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let events = differ.diff(&snap);
        assert!(events.iter().any(|e| matches!(
            e,
            ObservationEvent::StructuredRaw { raw, .. } if raw == "PAGE_STATE|queue"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            ObservationEvent::StructuredRaw { raw, .. } if raw == "PLAYER_NAME|PLAYER_1|Jesus"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            ObservationEvent::CardObserved { card_id: Some(id), .. } if id == "OP01-001"
        )));
    }

    #[test]
    fn combat_snapshot_emits_attack_with_sides() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        let snap = BrowserGameSnapshot {
            timestamp: 1,
            self_player: Some(BrowserPlayerSnapshot {
                leader_id: Some("ST01-001".into()),
                ..Default::default()
            }),
            opponent: Some(BrowserPlayerSnapshot {
                leader_id: Some("OP01-001".into()),
                board: vec![ObservedCard {
                    card_id: Some("ST01-012".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            combat: Some(BrowserCombatSnapshot {
                attacker: Some(ObservedCard {
                    card_id: Some("ST01-012".into()),
                    ..Default::default()
                }),
                target: Some(ObservedCard {
                    card_id: Some("ST01-001".into()),
                    ..Default::default()
                }),
                attacker_player: Some("opponent".into()),
                target_player: Some("self".into()),
                target_is_leader: Some(true),
                displayed_power: Some(6000),
                ..Default::default()
            }),
            ..Default::default()
        };
        let events = differ.diff(&snap);
        assert!(events.iter().any(|e| matches!(
            e,
            ObservationEvent::AttackObserved {
                attacker_card_id: Some(id),
                attacker_player: Some(PlayerId::Player2),
                target: Some(optcg_core::AttackTarget::Leader { player: PlayerId::Player1 }),
                ..
            } if id == "ST01-012"
        )));
    }

    #[test]
    fn combat_clearing_emits_resolve() {
        let mut differ = SnapshotDiffer::new(ConfidenceConfig::default());
        let fighting = BrowserGameSnapshot {
            timestamp: 1,
            combat: Some(BrowserCombatSnapshot {
                attacker: Some(ObservedCard {
                    card_id: Some("ST01-012".into()),
                    ..Default::default()
                }),
                displayed_power: Some(6000),
                ..Default::default()
            }),
            ..Default::default()
        };
        differ.diff(&fighting);
        let quiet = BrowserGameSnapshot {
            timestamp: 2,
            combat: None,
            ..Default::default()
        };
        let events = differ.diff(&quiet);
        assert!(events.iter().any(|e| matches!(
            e,
            ObservationEvent::StructuredRaw { raw, .. } if raw == "COMBAT_RESOLVED"
        )));
    }
}
