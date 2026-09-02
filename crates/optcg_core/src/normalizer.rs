use crate::error::{CoreError, CoreResult};
use crate::types::{
    CardInstance, CardState, CombatState, GameState, Phase, RawEvent, Zone,
};
use chrono::Utc;
use serde_json::Value;
use tracing::{debug, warn};

/// Parses raw log strings / JSON events and mutates `GameState` in place.
pub struct Normalizer;

impl Normalizer {
    pub fn apply_log_line(state: &mut GameState, line: &str) -> CoreResult<()> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        let event: RawEvent = if trimmed.starts_with('{') {
            serde_json::from_str(trimmed)?
        } else {
            Self::parse_plain_log(trimmed)?
        };

        Self::apply_event(state, &event)
    }

    pub fn apply_event(state: &mut GameState, event: &RawEvent) -> CoreResult<()> {
        state.connection.last_event_at = Some(Utc::now());
        state.connection.events_processed += 1;
        state.push_log(format!("{}: {}", event.event_type, event.payload));

        match event.event_type.as_str() {
            "PHASE_CHANGED" | "VISION_PHASE_HINT" => Self::handle_phase_changed(state, &event.payload),
            "DON_ATTACHED" | "DON_ATTACH" => Self::handle_don_attached(state, &event.payload),
            "CARD_PLAYED" | "PLAY_CARD" => Self::handle_card_played(state, &event.payload),
            "COMBAT_DECLARED" | "ATTACK_DECLARED" => {
                Self::handle_combat_declared(state, &event.payload)
            }
            "BLOCKER_OFFERED" | "BLOCKER_DECLARED" => {
                Self::handle_blocker_offered(state, &event.payload)
            }
            "COMBAT_RESOLVED" | "ATTACK_RESOLVED" => {
                Self::handle_combat_resolved(state, &event.payload)
            }
            "TURN_END" | "END_TURN" => Self::handle_turn_end(state, &event.payload),
            "LIFE_LOST" => Self::handle_life_lost(state, &event.payload),
            "CARD_KO" | "CHARACTER_KO" => Self::handle_card_ko(state, &event.payload),
            "DRAW_CARD" => Self::handle_draw(state, &event.payload),
            "REFRESH" | "STATE_SYNC" => Self::handle_state_sync(state, &event.payload),
            _ => {
                warn!(event_type = %event.event_type, "unknown event type, skipping");
                Err(CoreError::UnknownEvent(event.event_type.clone()))
            }
        }
    }

    fn parse_plain_log(line: &str) -> CoreResult<RawEvent> {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        let event_type = parts[0].trim().to_uppercase().replace(' ', "_");
        let payload = if parts.len() > 1 {
            serde_json::from_str(parts[1].trim()).unwrap_or_else(|_| {
                Value::Object(
                    [("raw".to_string(), Value::String(parts[1].trim().to_string()))]
                        .into_iter()
                        .collect(),
                )
            })
        } else {
            Value::Object(Default::default())
        };
        Ok(RawEvent {
            event_type,
            payload,
        })
    }

    fn player_index(payload: &Value, key: &str) -> CoreResult<u8> {
        payload
            .get(key)
            .and_then(|v| v.as_u64())
            .map(|n| n as u8)
            .ok_or_else(|| CoreError::InvalidPayload(format!("missing {key}")))
    }

    fn card_id(payload: &Value) -> CoreResult<String> {
        payload
            .get("card_id")
            .or_else(|| payload.get("attacker"))
            .or_else(|| payload.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| CoreError::InvalidPayload("missing card_id".into()))
    }

    fn handle_phase_changed(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        let phase_str = payload
            .get("phase")
            .and_then(|v| v.as_str())
            .unwrap_or("Main");
        state.phase = Phase::from_str_loose(phase_str);

        if let Ok(player) = Self::player_index(payload, "active_player") {
            state.active_player = player;
        }

        if state.phase != Phase::Combat {
            state.combat.active = false;
        }

        debug!(?state.phase, "phase changed");
        Ok(())
    }

    fn handle_don_attached(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        let player = Self::player_index(payload, "player")?;
        let amount = payload
            .get("amount")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;

        let card_id = payload.get("card_id").and_then(|v| v.as_str());

        if let Some(p) = state.player_mut(player) {
            if p.don_active >= amount {
                p.don_active -= amount;
            }
            if let Some(cid) = card_id {
                if let Some(character) = p.find_character(cid) {
                    character.attached_don += amount;
                } else if p.leader_id == cid {
                    // Leader DON attachment increases effective power via modifier
                    let leader = &mut p.leader_power;
                    *leader = leader.saturating_add(amount * 1000);
                }
            }
        }
        Ok(())
    }

    fn handle_card_played(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        let player = Self::player_index(payload, "player")?;
        let card_id = Self::card_id(payload)?;
        let zone_str = payload
            .get("zone")
            .and_then(|v| v.as_str())
            .unwrap_or("character");

        let zone = match zone_str.to_ascii_lowercase().as_str() {
            "stage" => Zone::Stage,
            "event" => Zone::Trash,
            _ => Zone::Character,
        };

        if let Some(p) = state.player_mut(player) {
            p.hand_count = p.hand_count.saturating_sub(1);
            let instance = CardInstance::new(card_id.clone(), player, zone);
            match zone {
                Zone::Stage => p.stage = Some(instance),
                Zone::Character => p.characters.push(instance),
                _ => p.trash.push(instance),
            }
        }
        Ok(())
    }

    fn handle_combat_declared(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        let attacker = payload
            .get("attacker")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let target = payload
            .get("target")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let target_player = Self::player_index(payload, "target_player").ok();
        let target_is_leader = target.as_deref() == Some("leader");

        state.phase = Phase::Combat;
        state.combat = CombatState {
            active: true,
            attacker_id: attacker,
            attacker_player: Some(state.active_player),
            target_id: target,
            target_player,
            target_is_leader,
            ..CombatState::default()
        };
        Ok(())
    }

    fn handle_blocker_offered(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        state.combat.blocker_offered = true;
        state.combat.blocker_id = payload
            .get("blocker_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        Ok(())
    }

    fn handle_combat_resolved(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        let damage = payload
            .get("damage")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let blocked = payload
            .get("blocked")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        state.combat.resolved = true;
        state.combat.damage = damage;
        state.combat.blocked = blocked;
        state.combat.active = false;

        if !blocked && state.combat.target_is_leader {
            if let Some(tp) = state.combat.target_player {
                if let Some(p) = state.player_mut(tp) {
                    if damage >= p.leader_power {
                        p.life = p.life.saturating_sub(1);
                    }
                }
            }
        }

        state.phase = Phase::Main;
        Ok(())
    }

    fn handle_turn_end(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        let next = Self::player_index(payload, "next_player").unwrap_or(1 - state.active_player);
        state.active_player = next;
        state.turn_number += 1;
        state.phase = Phase::Draw;
        state.combat.reset();

        for player in &mut state.players {
            player.don_active += 2.min(10 - player.total_don());
            for c in &mut player.characters {
                c.state = CardState::Ready;
                c.tapped = false;
            }
            player.leader_rested = false;
        }
        Ok(())
    }

    fn handle_life_lost(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        let player = Self::player_index(payload, "player")?;
        let amount = payload
            .get("amount")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;
        if let Some(p) = state.player_mut(player) {
            p.life = p.life.saturating_sub(amount);
        }
        Ok(())
    }

    fn handle_card_ko(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        let player = Self::player_index(payload, "player")?;
        let card_id = Self::card_id(payload)?;
        if let Some(p) = state.player_mut(player) {
            if let Some(pos) = p.characters.iter().position(|c| c.card_id == card_id) {
                let mut card = p.characters.remove(pos);
                card.state = CardState::KOd;
                card.zone = Zone::Trash;
                p.trash.push(card);
            }
        }
        Ok(())
    }

    fn handle_draw(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        let player = Self::player_index(payload, "player")?;
        let count = payload
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;
        if let Some(p) = state.player_mut(player) {
            p.hand_count += count;
            p.deck_count = p.deck_count.saturating_sub(count);
        }
        Ok(())
    }

    fn handle_state_sync(state: &mut GameState, payload: &Value) -> CoreResult<()> {
        if let Ok(parsed) = serde_json::from_value::<GameState>(payload.clone()) {
            *state = parsed;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn phase_changed_updates_state() {
        let mut state = GameState::new();
        let event = RawEvent {
            event_type: "PHASE_CHANGED".into(),
            payload: json!({"phase": "Main", "active_player": 0}),
        };
        Normalizer::apply_event(&mut state, &event).unwrap();
        assert_eq!(state.phase, Phase::Main);
    }

    #[test]
    fn combat_declared_sets_combat_state() {
        let mut state = GameState::new();
        let event = RawEvent {
            event_type: "COMBAT_DECLARED".into(),
            payload: json!({"attacker": "ST01-002", "target": "leader", "target_player": 1}),
        };
        Normalizer::apply_event(&mut state, &event).unwrap();
        assert!(state.combat.active);
        assert_eq!(state.combat.attacker_id.as_deref(), Some("ST01-002"));
    }
}
