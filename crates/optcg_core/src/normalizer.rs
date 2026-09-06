use crate::error::{CoreError, CoreResult};
use crate::events::{AttackTarget, GameEvent, LastEventInfo, PlayerId};
use crate::types::{CardInstance, CardState, CombatState, GameState, Phase, RawEvent, Zone};
use chrono::Utc;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tracing::{debug, info};

/// Parses raw external events and applies canonical `GameEvent` mutations.
pub struct Normalizer;

impl Normalizer {
    /// Full pipeline: parse → dedupe check → apply → sequence bump.
    pub fn process_raw(state: &mut GameState, raw: &str) -> CoreResult<LastEventInfo> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(CoreError::InvalidPayload("empty event".into()));
        }

        let event = Self::parse_event(trimmed)?;
        let fingerprint = Self::fingerprint(trimmed);

        if state.last_processed_fingerprint.as_deref() == Some(&fingerprint) {
            return Err(CoreError::DuplicateEvent(fingerprint));
        }

        Self::apply_event(state, &event)?;

        state.event_sequence += 1;
        state.timestamp = Utc::now();
        state.connection.last_event_at = Some(state.timestamp);
        state.connection.events_processed += 1;
        state.last_processed_fingerprint = Some(fingerprint.clone());

        let info = LastEventInfo {
            sequence: state.event_sequence,
            event_name: event.name().to_string(),
            summary: Self::summarize(&event),
            processed_at: state.timestamp,
        };
        state.last_event = Some(info.clone());
        state.push_log(format!("#{} {}", info.sequence, info.summary));

        info!(
            target: "optcg::event",
            seq = info.sequence,
            event = %info.event_name,
            "event processed"
        );
        debug!(target: "optcg::state", phase = ?state.phase, turn = state.turn_number, "state updated");

        Ok(info)
    }

    /// Back-compat entry point used by legacy call sites.
    pub fn apply_log_line(state: &mut GameState, line: &str) -> CoreResult<()> {
        Self::process_raw(state, line).map(|_| ())
    }

    /// Parse raw simulator input into a strongly typed `GameEvent`.
    pub fn parse_event(raw: &str) -> CoreResult<GameEvent> {
        let trimmed = raw.trim();
        if trimmed.starts_with('{') {
            Self::parse_json_event(trimmed)
        } else if trimmed.contains('|') {
            Self::parse_pipe_event(trimmed)
        } else {
            Self::parse_legacy_plain(trimmed)
        }
    }

    /// Apply a canonical event to game state (no sequencing side effects).
    pub fn apply_event(state: &mut GameState, event: &GameEvent) -> CoreResult<()> {
        match event {
            GameEvent::GameStarted => {
                *state = GameState::new();
                state.connection.status = crate::types::ConnectionStatus::Connected;
            }
            GameEvent::PhaseChanged { phase } => {
                state.phase = *phase;
                if *phase != Phase::Combat {
                    state.combat.active = false;
                }
            }
            GameEvent::TurnStarted { player } => {
                state.active_player = player.index();
                state.phase = Phase::Draw;
            }
            GameEvent::TurnEnded { next_player } => {
                state.active_player = next_player.index();
                state.turn_number += 1;
                state.phase = Phase::Draw;
                state.combat.reset();
                for player in &mut state.players {
                    player.don_active += 2.min(10 - player.total_don());
                    for c in &mut player.characters {
                        c.set_rested(false);
                    }
                    player.leader.rested = false;
                    player.sync_leader_fields();
                }
            }
            GameEvent::CardPlayed {
                player,
                card_id,
                zone,
            } => Self::apply_card_played(state, *player, card_id, zone.as_deref())?,
            GameEvent::DonAttached {
                player,
                target,
                amount,
            } => Self::apply_don_attached(state, *player, target, *amount)?,
            GameEvent::AttackDeclared {
                attacker,
                attacker_player,
                target,
                power,
            } => Self::apply_attack_declared(state, attacker, *attacker_player, target, *power)?,
            GameEvent::BlockerActivated {
                player: _,
                card_instance,
            } => {
                state.combat.blocker_offered = true;
                state.combat.blocker_id = Some(card_instance.clone());
                state.combat.blocked = true;
            }
            GameEvent::BlockerOffered { blocker_id, .. } => {
                state.combat.blocker_offered = true;
                state.combat.blocker_id = Some(blocker_id.clone());
            }
            GameEvent::CombatResolved { damage, blocked } => {
                Self::apply_combat_resolved(state, *damage, *blocked)?;
            }
            GameEvent::LifeChanged { player, delta } => {
                if let Some(p) = state.player_mut(player.index()) {
                    if *delta < 0 {
                        p.life = p.life.saturating_sub((-delta) as u32);
                    } else {
                        p.life += *delta as u32;
                    }
                }
            }
            GameEvent::CardAddedToHand { player, card_id } => {
                if let Some(p) = state.player_mut(player.index()) {
                    p.hand_count += 1;
                    p.hand.push(CardInstance::new(
                        card_id.clone(),
                        player.index(),
                        Zone::Hand,
                    ));
                }
            }
            GameEvent::CardRemovedFromBoard {
                player,
                card_instance,
            } => Self::apply_card_removed(state, *player, card_instance)?,
            GameEvent::DrawCard { player, count } => {
                if let Some(p) = state.player_mut(player.index()) {
                    p.hand_count += *count as u32;
                    p.deck_count = p.deck_count.saturating_sub(*count as u32);
                }
            }
            GameEvent::VisionPhaseHint { phase } => {
                state.phase = *phase;
            }
            GameEvent::StateSync { payload } => {
                if let Ok(parsed) = serde_json::from_value::<GameState>(payload.clone()) {
                    *state = parsed;
                }
            }
        }
        Ok(())
    }

    fn parse_json_event(raw: &str) -> CoreResult<GameEvent> {
        let envelope: RawEvent = serde_json::from_str(raw)?;
        if let Some(nested) = envelope.payload.get("raw").and_then(|v| v.as_str()) {
            return Self::parse_event(nested);
        }
        Self::raw_event_to_game_event(&envelope)
    }

    fn parse_pipe_event(raw: &str) -> CoreResult<GameEvent> {
        let parts: Vec<&str> = raw.split('|').map(str::trim).collect();
        if parts.is_empty() {
            return Err(CoreError::InvalidPayload("empty pipe event".into()));
        }
        let kind = parts[0].to_ascii_uppercase();
        match kind.as_str() {
            "GAME_STARTED" => Ok(GameEvent::GameStarted),
            "PHASE_CHANGED" => {
                let phase = Phase::from_str_loose(parts.get(1).copied().unwrap_or("MAIN"));
                Ok(GameEvent::PhaseChanged { phase })
            }
            "TURN_STARTED" => {
                let player = PlayerId::parse_token(parts.get(1).copied().unwrap_or("PLAYER_1"))?;
                Ok(GameEvent::TurnStarted { player })
            }
            "TURN_ENDED" | "TURN_END" => {
                let next = PlayerId::parse_token(parts.get(1).copied().unwrap_or("PLAYER_2"))?;
                Ok(GameEvent::TurnEnded { next_player: next })
            }
            "DON_ATTACHED" => {
                let player = PlayerId::parse_token(parts.get(1).copied().unwrap_or("PLAYER_1"))?;
                let target = parts.get(2).copied().unwrap_or("LEADER").to_string();
                let amount: u8 = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
                Ok(GameEvent::DonAttached {
                    player,
                    target,
                    amount,
                })
            }
            "CARD_PLAYED" | "PLAY_CARD" => {
                let player = PlayerId::parse_token(parts.get(1).copied().unwrap_or("PLAYER_1"))?;
                let card_id = parts
                    .get(2)
                    .ok_or_else(|| CoreError::InvalidPayload("missing card_id".into()))?
                    .to_string();
                let zone = parts.get(3).map(|s| s.to_string());
                Ok(GameEvent::CardPlayed {
                    player,
                    card_id,
                    zone,
                })
            }
            "ATTACK_DECLARED" | "COMBAT_DECLARED" => {
                let attacker_player =
                    PlayerId::parse_token(parts.get(1).copied().unwrap_or("PLAYER_1"))?;
                let attacker = parts
                    .get(2)
                    .ok_or_else(|| CoreError::InvalidPayload("missing attacker".into()))?
                    .to_string();
                let target_token = parts.get(3).copied().unwrap_or("LEADER");
                let target = if target_token.eq_ignore_ascii_case("LEADER") {
                    let defender = parts
                        .get(4)
                        .and_then(|s| PlayerId::parse_token(s).ok())
                        .unwrap_or(attacker_player.opponent());
                    AttackTarget::Leader { player: defender }
                } else if target_token.starts_with("PLAYER_") {
                    AttackTarget::Leader {
                        player: PlayerId::parse_token(target_token)?,
                    }
                } else {
                    let defender = parts
                        .get(4)
                        .and_then(|s| PlayerId::parse_token(s).ok())
                        .unwrap_or(attacker_player.opponent());
                    AttackTarget::Character {
                        player: defender,
                        card_id: target_token.to_string(),
                    }
                };
                let power: u32 = parts.iter().rev().find_map(|s| s.parse().ok()).unwrap_or(0);
                Ok(GameEvent::AttackDeclared {
                    attacker,
                    attacker_player,
                    target,
                    power,
                })
            }
            "BLOCKER_ACTIVATED" | "BLOCKER_OFFERED" => {
                let player = PlayerId::parse_token(parts.get(1).copied().unwrap_or("PLAYER_2"))?;
                let card = parts
                    .get(2)
                    .ok_or_else(|| CoreError::InvalidPayload("missing blocker id".into()))?
                    .to_string();
                if kind == "BLOCKER_ACTIVATED" {
                    Ok(GameEvent::BlockerActivated {
                        player,
                        card_instance: card,
                    })
                } else {
                    Ok(GameEvent::BlockerOffered {
                        player,
                        blocker_id: card,
                    })
                }
            }
            "COMBAT_RESOLVED" | "ATTACK_RESOLVED" => Ok(GameEvent::CombatResolved {
                damage: parts.get(1).and_then(|s| s.parse().ok()),
                blocked: parts
                    .get(2)
                    .map(|s| s.eq_ignore_ascii_case("true") || *s == "1"),
            }),
            "LIFE_CHANGED" | "LIFE_LOST" => {
                let player = PlayerId::parse_token(parts.get(1).copied().unwrap_or("PLAYER_2"))?;
                let delta: i8 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(-1);
                Ok(GameEvent::LifeChanged { player, delta })
            }
            "DRAW_CARD" => {
                let player = PlayerId::parse_token(parts.get(1).copied().unwrap_or("PLAYER_1"))?;
                let count: u8 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
                Ok(GameEvent::DrawCard { player, count })
            }
            other => Err(CoreError::UnknownEvent(other.to_string())),
        }
    }

    fn parse_legacy_plain(raw: &str) -> CoreResult<GameEvent> {
        let parts: Vec<&str> = raw.splitn(2, ':').collect();
        let event_type = parts[0].trim().to_uppercase().replace(' ', "_");
        let payload = if parts.len() > 1 {
            serde_json::from_str(parts[1].trim()).unwrap_or_else(|_| {
                Value::Object(
                    [(
                        "raw".to_string(),
                        Value::String(parts[1].trim().to_string()),
                    )]
                    .into_iter()
                    .collect(),
                )
            })
        } else {
            Value::Object(Default::default())
        };
        Self::raw_event_to_game_event(&RawEvent {
            event_type,
            payload,
        })
    }

    fn raw_event_to_game_event(raw: &RawEvent) -> CoreResult<GameEvent> {
        match raw.event_type.as_str() {
            "GAME_STARTED" => Ok(GameEvent::GameStarted),
            "PHASE_CHANGED" | "VISION_PHASE_HINT" => {
                let phase_str = raw
                    .payload
                    .get("phase")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Main");
                Ok(GameEvent::PhaseChanged {
                    phase: Phase::from_str_loose(phase_str),
                })
            }
            "TURN_STARTED" => {
                let player = Self::player_from_payload(&raw.payload, "player")
                    .or_else(|_| Self::player_from_payload(&raw.payload, "active_player"))?;
                Ok(GameEvent::TurnStarted { player })
            }
            "TURN_END" | "TURN_ENDED" | "END_TURN" => {
                let next = Self::player_from_payload(&raw.payload, "next_player")
                    .unwrap_or(PlayerId::Player2);
                Ok(GameEvent::TurnEnded { next_player: next })
            }
            "DON_ATTACHED" | "DON_ATTACH" => {
                let player = Self::player_from_payload(&raw.payload, "player")?;
                let target = raw
                    .payload
                    .get("card_id")
                    .or_else(|| raw.payload.get("target"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("LEADER")
                    .to_string();
                let amount = raw
                    .payload
                    .get("amount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u8;
                Ok(GameEvent::DonAttached {
                    player,
                    target,
                    amount,
                })
            }
            "CARD_PLAYED" | "PLAY_CARD" => {
                let player = Self::player_from_payload(&raw.payload, "player")?;
                let card_id = Self::card_id_from_payload(&raw.payload)?;
                let zone = raw
                    .payload
                    .get("zone")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                Ok(GameEvent::CardPlayed {
                    player,
                    card_id,
                    zone,
                })
            }
            "COMBAT_DECLARED" | "ATTACK_DECLARED" => {
                let attacker_player = Self::player_from_payload(&raw.payload, "attacker_player")
                    .or_else(|_| Self::player_from_payload(&raw.payload, "player"))
                    .unwrap_or(PlayerId::Player1);
                let attacker = raw
                    .payload
                    .get("attacker")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| CoreError::InvalidPayload("missing attacker".into()))?;
                let target_str = raw
                    .payload
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("leader");
                let target_player = Self::player_from_payload(&raw.payload, "target_player")
                    .unwrap_or(attacker_player.opponent());
                let power = raw
                    .payload
                    .get("power")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let target = if target_str.eq_ignore_ascii_case("leader") {
                    AttackTarget::Leader {
                        player: target_player,
                    }
                } else {
                    AttackTarget::Character {
                        player: target_player,
                        card_id: target_str.to_string(),
                    }
                };
                Ok(GameEvent::AttackDeclared {
                    attacker,
                    attacker_player,
                    target,
                    power,
                })
            }
            "BLOCKER_OFFERED" | "BLOCKER_DECLARED" => {
                let player = Self::player_from_payload(&raw.payload, "player")?;
                let blocker_id = raw
                    .payload
                    .get("blocker_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(GameEvent::BlockerOffered { player, blocker_id })
            }
            "BLOCKER_ACTIVATED" => {
                let player = Self::player_from_payload(&raw.payload, "player")?;
                let card_instance = raw
                    .payload
                    .get("card_instance")
                    .or_else(|| raw.payload.get("blocker_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(GameEvent::BlockerActivated {
                    player,
                    card_instance,
                })
            }
            "COMBAT_RESOLVED" | "ATTACK_RESOLVED" => Ok(GameEvent::CombatResolved {
                damage: raw
                    .payload
                    .get("damage")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32),
                blocked: raw.payload.get("blocked").and_then(|v| v.as_bool()),
            }),
            "LIFE_LOST" | "LIFE_CHANGED" => {
                let player = Self::player_from_payload(&raw.payload, "player")?;
                let delta = if raw.event_type == "LIFE_LOST" {
                    -(raw
                        .payload
                        .get("amount")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(1) as i8)
                } else {
                    raw.payload
                        .get("delta")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(-1) as i8
                };
                Ok(GameEvent::LifeChanged { player, delta })
            }
            "CARD_KO" | "CHARACTER_KO" => {
                let player = Self::player_from_payload(&raw.payload, "player")?;
                let card_instance = Self::card_id_from_payload(&raw.payload)?;
                Ok(GameEvent::CardRemovedFromBoard {
                    player,
                    card_instance,
                })
            }
            "DRAW_CARD" => {
                let player = Self::player_from_payload(&raw.payload, "player")?;
                let count = raw
                    .payload
                    .get("count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u8;
                Ok(GameEvent::DrawCard { player, count })
            }
            "REFRESH" | "STATE_SYNC" => Ok(GameEvent::StateSync {
                payload: raw.payload.clone(),
            }),
            other => Err(CoreError::UnknownEvent(other.to_string())),
        }
    }

    fn apply_card_played(
        state: &mut GameState,
        player: PlayerId,
        card_id: &str,
        zone: Option<&str>,
    ) -> CoreResult<()> {
        let zone_key = zone.unwrap_or("character").to_ascii_lowercase();
        if let Some(p) = state.player_mut(player.index()) {
            p.note_card(card_id);
            if zone_key.contains("leader") {
                p.set_leader_id(card_id);
                return Ok(());
            }
            let zone = match zone_key.as_str() {
                "stage" => Zone::Stage,
                "event" | "trash" => Zone::Trash,
                "hand" => Zone::Hand,
                _ => Zone::Character,
            };
            if zone != Zone::Hand {
                p.hand_count = p.hand_count.saturating_sub(1);
            }
            let mut instance = CardInstance::new(card_id, player.index(), zone);
            instance.position = p.characters.len() as u8;
            match zone {
                Zone::Stage => p.stage = Some(instance),
                Zone::Character => {
                    if !p.characters.iter().any(|c| c.card_id == card_id && c.zone == Zone::Character) {
                        // Allow duplicates as separate instances when observed again with different keys upstream
                    }
                    p.characters.push(instance);
                }
                Zone::Hand => p.hand.push(instance),
                _ => p.push_trash(instance),
            }
        }
        Ok(())
    }

    fn apply_don_attached(
        state: &mut GameState,
        player: PlayerId,
        target: &str,
        amount: u8,
    ) -> CoreResult<()> {
        let amount = amount as u32;
        if let Some(p) = state.player_mut(player.index()) {
            if p.don_active >= amount {
                p.don_active -= amount;
            }
            if target.eq_ignore_ascii_case("LEADER") || p.leader.card_id == target {
                p.leader.attached_don += amount;
                p.leader.power = p.leader.power.saturating_add(amount * 1000);
            } else if let Some(character) = p.find_character(target) {
                character.attached_don += amount;
            }
            p.sync_leader_fields();
        }
        Ok(())
    }

    fn apply_attack_declared(
        state: &mut GameState,
        attacker: &str,
        attacker_player: PlayerId,
        target: &AttackTarget,
        power: u32,
    ) -> CoreResult<()> {
        state.phase = Phase::Combat;
        let (target_id, target_player, target_is_leader) = match target {
            AttackTarget::Leader { player } => {
                (Some("leader".to_string()), Some(player.index()), true)
            }
            AttackTarget::Character { player, card_id } => {
                (Some(card_id.clone()), Some(player.index()), false)
            }
        };
        state.combat = CombatState {
            active: true,
            attacker_id: Some(attacker.to_string()),
            attacker_player: Some(attacker_player.index()),
            target_id,
            target_player,
            target_is_leader,
            ..CombatState::default()
        };
        if power > 0 {
            if let Some(p) = state.player_mut(attacker_player.index()) {
                if let Some(c) = p.find_character(attacker) {
                    c.power_modifier += power as i32;
                }
            }
        }
        Ok(())
    }

    fn apply_combat_resolved(
        state: &mut GameState,
        damage: Option<u32>,
        blocked: Option<bool>,
    ) -> CoreResult<()> {
        let damage = damage.unwrap_or(0);
        let blocked = blocked.unwrap_or(false);
        state.combat.resolved = true;
        state.combat.damage = damage;
        state.combat.blocked = blocked;
        state.combat.active = false;

        if !blocked && state.combat.target_is_leader {
            if let Some(tp) = state.combat.target_player {
                if let Some(p) = state.player_mut(tp) {
                    if damage >= p.leader.effective_power() {
                        p.life = p.life.saturating_sub(1);
                    }
                }
            }
        }
        state.phase = Phase::Main;
        Ok(())
    }

    fn apply_card_removed(
        state: &mut GameState,
        player: PlayerId,
        card_instance: &str,
    ) -> CoreResult<()> {
        if let Some(p) = state.player_mut(player.index()) {
            if let Some(pos) = p.characters.iter().position(|c| c.card_id == card_instance) {
                let mut card = p.characters.remove(pos);
                card.state = CardState::KOd;
                card.zone = Zone::Trash;
                p.push_trash(card);
            }
        }
        Ok(())
    }

    fn player_from_payload(payload: &Value, key: &str) -> CoreResult<PlayerId> {
        if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
            return PlayerId::parse_token(s);
        }
        payload
            .get(key)
            .and_then(|v| v.as_u64())
            .map(|n| n as u8)
            .ok_or_else(|| CoreError::InvalidPayload(format!("missing {key}")))
            .and_then(PlayerId::from_index)
    }

    fn card_id_from_payload(payload: &Value) -> CoreResult<String> {
        payload
            .get("card_id")
            .or_else(|| payload.get("attacker"))
            .or_else(|| payload.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| CoreError::InvalidPayload("missing card_id".into()))
    }

    fn fingerprint(raw: &str) -> String {
        let mut hasher = DefaultHasher::new();
        raw.trim().hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// One-line description of an event, for the action log and the HUD.
    pub fn summarize(event: &GameEvent) -> String {
        match event {
            GameEvent::PhaseChanged { phase } => format!("PHASE_CHANGED {:?}", phase),
            GameEvent::AttackDeclared {
                attacker, power, ..
            } => {
                format!("ATTACK_DECLARED {attacker} power={power}")
            }
            GameEvent::LifeChanged { player, delta } => {
                format!("LIFE_CHANGED {:?} delta={delta}", player)
            }
            other => other.name().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Phase;

    #[test]
    fn parse_pipe_phase_changed() {
        let event = Normalizer::parse_event("PHASE_CHANGED|MAIN").unwrap();
        assert!(matches!(
            event,
            GameEvent::PhaseChanged { phase: Phase::Main }
        ));
    }

    #[test]
    fn parse_pipe_don_attached() {
        let event = Normalizer::parse_event("DON_ATTACHED|PLAYER_1|LEADER|1").unwrap();
        assert!(matches!(event, GameEvent::DonAttached { amount: 1, .. }));
    }

    #[test]
    fn sequential_events_increment_sequence() {
        let mut state = GameState::new();
        Normalizer::process_raw(&mut state, "PHASE_CHANGED|MAIN").unwrap();
        Normalizer::process_raw(&mut state, "TURN_STARTED|PLAYER_1").unwrap();
        assert_eq!(state.event_sequence, 2);
        assert_eq!(state.phase, Phase::Draw);
        assert_eq!(state.active_player, 0);
    }

    #[test]
    fn duplicate_event_rejected() {
        let mut state = GameState::new();
        Normalizer::process_raw(&mut state, "PHASE_CHANGED|MAIN").unwrap();
        let err = Normalizer::process_raw(&mut state, "PHASE_CHANGED|MAIN").unwrap_err();
        assert!(matches!(err, CoreError::DuplicateEvent(_)));
    }

    #[test]
    fn malformed_event_returns_error() {
        let err = Normalizer::parse_event("UNKNOWN_EVENT|foo").unwrap_err();
        assert!(matches!(err, CoreError::UnknownEvent(_)));
    }

    #[test]
    fn invalid_player_id_rejected() {
        let err = Normalizer::parse_event("TURN_STARTED|PLAYER_99").unwrap_err();
        assert!(matches!(err, CoreError::InvalidPayload(_)));
    }

    #[test]
    fn life_changed_mutates_state() {
        let mut state = GameState::new();
        Normalizer::process_raw(&mut state, "LIFE_CHANGED|PLAYER_2|-1").unwrap();
        assert_eq!(state.player_two().life, 4);
    }

    #[test]
    fn attack_declared_sets_combat() {
        let mut state = GameState::new();
        Normalizer::process_raw(
            &mut state,
            "ATTACK_DECLARED|PLAYER_1|ST01-002|LEADER|PLAYER_2|6000",
        )
        .unwrap();
        assert!(state.combat.active);
    }
}
