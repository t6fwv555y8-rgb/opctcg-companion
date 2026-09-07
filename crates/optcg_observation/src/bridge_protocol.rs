use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Normalized browser-visible game snapshot (never includes hidden info).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BrowserGameSnapshot {
    pub timestamp: i64,
    pub source: Option<String>,
    pub sequence: Option<u64>,
    pub turn: Option<u32>,
    pub phase: Option<String>,
    pub active_player: Option<String>,
    /// `queue`, `lobby`, or `match` when the page can say where the player is.
    #[serde(default)]
    pub page_state: Option<String>,
    #[serde(rename = "self")]
    pub self_player: Option<BrowserPlayerSnapshot>,
    pub opponent: Option<BrowserPlayerSnapshot>,
    pub combat: Option<BrowserCombatSnapshot>,
    pub diagnostics: Option<AdapterDiagnostics>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BrowserPlayerSnapshot {
    pub life: Option<u8>,
    pub hand_count: Option<u8>,
    pub active_don: Option<u8>,
    pub rested_don: Option<u8>,
    /// Leader card ID when visible.
    pub leader_id: Option<String>,
    /// Simulator display name for this player, when the page shows one.
    #[serde(default)]
    pub player_name: Option<String>,
    /// Deck name from UI when detectable.
    pub deck_name: Option<String>,
    /// Unique known card IDs for this player (visible zones only).
    #[serde(default)]
    pub known_cards: Vec<String>,
    #[serde(default)]
    pub board: Vec<ObservedCard>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ObservedCard {
    pub card_id: Option<String>,
    pub instance_key: Option<String>,
    pub name: Option<String>,
    pub power: Option<u32>,
    pub rested: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BrowserCombatSnapshot {
    pub attacker: Option<ObservedCard>,
    pub target: Option<ObservedCard>,
    pub displayed_power: Option<u32>,
    /// `self` / `opponent` when the page can say who owns the attacker.
    #[serde(default)]
    pub attacker_player: Option<String>,
    /// `self` / `opponent` when the page can say who is being attacked.
    #[serde(default)]
    pub target_player: Option<String>,
    #[serde(default)]
    pub target_is_leader: Option<bool>,
    #[serde(default)]
    pub blocker_offered: Option<bool>,
    #[serde(default)]
    pub blocker_id: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AdapterDiagnostics {
    pub site_detected: Option<bool>,
    pub game_detected: Option<bool>,
    pub ui_recognized: Option<bool>,
    pub message: Option<String>,
    #[serde(default)]
    pub found: HashMap<String, bool>,
}

/// Wire protocol message from browser companion extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeMessage {
    #[serde(rename = "snapshot")]
    Snapshot(BrowserGameSnapshot),
    Ping,
    Pong,
}

pub const MAX_BRIDGE_PAYLOAD: usize = 256 * 1024;

pub fn validate_bridge_payload(bytes: &[u8]) -> Result<BridgeMessage, String> {
    if bytes.len() > MAX_BRIDGE_PAYLOAD {
        return Err(format!("payload exceeds {} bytes", MAX_BRIDGE_PAYLOAD));
    }
    serde_json::from_slice(bytes).map_err(|e| format!("malformed json: {e}"))
}

/// Parse a raw browser snapshot (HTTP POST body) or wrapped bridge message.
pub fn parse_snapshot_payload(bytes: &[u8]) -> Result<BrowserGameSnapshot, String> {
    if bytes.len() > MAX_BRIDGE_PAYLOAD {
        return Err(format!("payload exceeds {} bytes", MAX_BRIDGE_PAYLOAD));
    }
    if let Ok(msg) = serde_json::from_slice::<BridgeMessage>(bytes) {
        return match msg {
            BridgeMessage::Snapshot(snap) => Ok(snap),
            _ => Err("expected snapshot message".into()),
        };
    }
    serde_json::from_slice(bytes).map_err(|e| format!("malformed snapshot json: {e}"))
}
