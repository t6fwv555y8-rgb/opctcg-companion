use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Game phase in the OPTCG turn structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum Phase {
    #[default]
    Draw,
    Don,
    Main,
    End,
    Combat,
}

impl Phase {
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "draw" => Phase::Draw,
            "don" | "don!!" => Phase::Don,
            "main" => Phase::Main,
            "end" => Phase::End,
            "combat" | "battle" => Phase::Combat,
            _ => Phase::Main,
        }
    }
}

/// Orientation of a card on the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CardOrientation {
    #[default]
    Active,
    Rested,
}

/// Zone a card instance occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    #[default]
    Hand,
    Deck,
    Character,
    Stage,
    Leader,
    Life,
    Trash,
    DonDeck,
    DonArea,
    CostArea,
}

/// Lifecycle state of a card instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CardState {
    #[default]
    Ready,
    Rested,
    KOd,
    Triggered,
}

/// Card type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CardType {
    #[default]
    Character,
    Event,
    Stage,
    Leader,
}

/// Keyword abilities relevant to rules evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct Keywords {
    pub rush: bool,
    pub blocker: bool,
    pub double_attack: bool,
    pub banish: bool,
    pub counter: i32,
}

impl Keywords {
    pub fn has_blocker(&self) -> bool {
        self.blocker
    }

    pub fn counter_value(&self) -> i32 {
        self.counter.max(0)
    }
}

/// Static card definition reference (resolved via database).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardDefinition {
    pub card_id: String,
    pub name: String,
    pub card_type: CardType,
    pub cost: u32,
    pub power: u32,
    pub counter: i32,
    pub color: String,
    pub keywords: Keywords,
    pub text: String,
}

/// Runtime card instance on the battlefield or in zones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardInstance {
    pub instance_id: Uuid,
    pub card_id: String,
    pub owner: u8,
    pub zone: Zone,
    pub state: CardState,
    pub orientation: CardOrientation,
    pub attached_don: u32,
    pub power_modifier: i32,
    pub tapped: bool,
}

impl CardInstance {
    pub fn new(card_id: impl Into<String>, owner: u8, zone: Zone) -> Self {
        Self {
            instance_id: Uuid::new_v4(),
            card_id: card_id.into(),
            owner,
            zone,
            state: CardState::Ready,
            orientation: CardOrientation::Active,
            attached_don: 0,
            power_modifier: 0,
            tapped: false,
        }
    }

    pub fn effective_power(&self, base_power: u32) -> i32 {
        base_power as i32 + self.power_modifier + (self.attached_don as i32 * 1000)
    }
}

/// Per-player runtime state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub player_index: u8,
    pub life: u32,
    pub don_active: u32,
    pub don_rested: u32,
    pub hand_count: u32,
    pub deck_count: u32,
    pub leader_id: String,
    pub leader_power: u32,
    pub leader_rested: bool,
    pub characters: Vec<CardInstance>,
    pub stage: Option<CardInstance>,
    pub hand: Vec<CardInstance>,
    pub trash: Vec<CardInstance>,
}

impl PlayerState {
    pub fn new(index: u8, leader_id: impl Into<String>) -> Self {
        Self {
            player_index: index,
            life: 5,
            don_active: 0,
            don_rested: 0,
            hand_count: 5,
            deck_count: 45,
            leader_id: leader_id.into(),
            leader_power: 5000,
            leader_rested: false,
            characters: Vec::new(),
            stage: None,
            hand: Vec::new(),
            trash: Vec::new(),
        }
    }

    pub fn total_don(&self) -> u32 {
        self.don_active + self.don_rested
    }

    pub fn find_character(&mut self, card_id: &str) -> Option<&mut CardInstance> {
        self.characters
            .iter_mut()
            .find(|c| c.card_id == card_id)
    }

    pub fn find_blocker(&self) -> Option<&CardInstance> {
        self.characters.iter().find(|c| {
            c.state == CardState::Ready
                && c.zone == Zone::Character
                && !c.tapped
        })
    }
}

/// Active combat interaction snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CombatState {
    pub active: bool,
    pub attacker_id: Option<String>,
    pub attacker_player: Option<u8>,
    pub target_id: Option<String>,
    pub target_player: Option<u8>,
    pub target_is_leader: bool,
    pub blocker_offered: bool,
    pub blocker_id: Option<String>,
    pub resolved: bool,
    pub damage: u32,
    pub blocked: bool,
}

impl CombatState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Connectivity and ingestion metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConnectionState {
    pub websocket_connected: bool,
    pub file_monitor_active: bool,
    pub last_event_at: Option<DateTime<Utc>>,
    pub events_processed: u64,
    pub latency_ms: u64,
}

/// Top-level normalized game state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    pub match_id: Uuid,
    pub active_player: u8,
    pub turn_number: u32,
    pub phase: Phase,
    pub players: [PlayerState; 2],
    pub combat: CombatState,
    pub connection: ConnectionState,
    pub event_log: Vec<String>,
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

impl GameState {
    pub fn new() -> Self {
        Self {
            match_id: Uuid::new_v4(),
            active_player: 0,
            turn_number: 1,
            phase: Phase::Draw,
            players: [
                PlayerState::new(0, "ST01-001"),
                PlayerState::new(1, "ST01-001"),
            ],
            combat: CombatState::default(),
            connection: ConnectionState::default(),
            event_log: Vec::new(),
        }
    }

    pub fn active_player_mut(&mut self) -> &mut PlayerState {
        &mut self.players[self.active_player as usize]
    }

    pub fn opponent(&self) -> &PlayerState {
        let idx = 1 - self.active_player as usize;
        &self.players[idx]
    }

    pub fn opponent_mut(&mut self) -> &mut PlayerState {
        let idx = 1 - self.active_player as usize;
        &mut self.players[idx]
    }

    pub fn player_mut(&mut self, index: u8) -> Option<&mut PlayerState> {
        self.players.get_mut(index as usize)
    }

    pub fn push_log(&mut self, entry: impl Into<String>) {
        self.event_log.push(entry.into());
        if self.event_log.len() > 500 {
            self.event_log.drain(0..100);
        }
    }
}

/// Raw event envelope from simulator stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub payload: serde_json::Value,
}
