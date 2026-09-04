use crate::events::LastEventInfo;
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

/// Static card attributes from database (never mixed with runtime state).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CardAttributes {
    pub color: String,
    pub card_type: CardType,
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
    pub attributes: CardAttributes,
    pub keywords: Keywords,
    #[serde(alias = "text")]
    pub rules_text: String,
}

impl CardDefinition {
    pub fn from_parts(
        card_id: String,
        name: String,
        card_type: CardType,
        cost: u32,
        power: u32,
        counter: i32,
        color: String,
        keywords: Keywords,
        rules_text: String,
    ) -> Self {
        Self {
            attributes: CardAttributes {
                color: color.clone(),
                card_type,
            },
            card_id,
            name,
            card_type,
            cost,
            power,
            counter,
            color,
            keywords,
            rules_text,
        }
    }
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
    pub rested: bool,
    pub known: bool,
    pub revealed: bool,
    pub position: u8,
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
            rested: false,
            known: true,
            revealed: zone != Zone::Hand,
            position: 0,
        }
    }

    pub fn set_rested(&mut self, rested: bool) {
        self.rested = rested;
        self.tapped = rested;
        self.orientation = if rested {
            CardOrientation::Rested
        } else {
            CardOrientation::Active
        };
        self.state = if rested {
            CardState::Rested
        } else {
            CardState::Ready
        };
    }

    pub fn effective_power(&self, base_power: u32) -> i32 {
        base_power as i32 + self.power_modifier + (self.attached_don as i32 * 1000)
    }
}

/// Leader runtime snapshot (distinct from static CardDefinition).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderState {
    pub card_id: String,
    pub power: u32,
    pub rested: bool,
    pub attached_don: u32,
}

impl LeaderState {
    pub fn new(card_id: impl Into<String>) -> Self {
        Self {
            card_id: card_id.into(),
            power: 5000,
            rested: false,
            attached_don: 0,
        }
    }

    pub fn effective_power(&self) -> u32 {
        self.power
            .saturating_add(self.attached_don.saturating_mul(1000))
    }
}

/// Per-player runtime state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub player_index: u8,
    pub leader: LeaderState,
    pub life: u32,
    pub don_active: u32,
    pub don_rested: u32,
    pub hand_count: u32,
    pub deck_count: u32,
    pub trash_count: u32,
    /// Board characters (alias: `board()` accessor).
    pub characters: Vec<CardInstance>,
    pub stage: Option<CardInstance>,
    pub hand: Vec<CardInstance>,
    pub trash: Vec<CardInstance>,
    /// Optional deck label from simulator UI (when visible).
    #[serde(default)]
    pub deck_name: String,
    /// Unique card IDs observed for this player this match (leader/board/self-hand/trash).
    #[serde(default)]
    pub known_cards: Vec<String>,
    // Legacy flat leader fields — kept for serialization compat
    #[serde(default)]
    pub leader_id: String,
    #[serde(default = "default_leader_power")]
    pub leader_power: u32,
    #[serde(default)]
    pub leader_rested: bool,
}

fn default_leader_power() -> u32 {
    5000
}

impl PlayerState {
    pub fn new(index: u8, leader_id: impl Into<String>) -> Self {
        let leader_id = leader_id.into();
        Self {
            player_index: index,
            leader: LeaderState::new(leader_id.clone()),
            life: 5,
            don_active: 0,
            don_rested: 0,
            hand_count: 5,
            deck_count: 45,
            trash_count: 0,
            characters: Vec::new(),
            stage: None,
            hand: Vec::new(),
            trash: Vec::new(),
            deck_name: String::new(),
            known_cards: Vec::new(),
            leader_id,
            leader_power: 5000,
            leader_rested: false,
        }
    }

    pub fn note_card(&mut self, card_id: &str) {
        if card_id.is_empty() {
            return;
        }
        if !self.known_cards.iter().any(|c| c == card_id) {
            self.known_cards.push(card_id.to_string());
        }
    }

    pub fn set_leader_id(&mut self, card_id: impl Into<String>) {
        let id = card_id.into();
        if id.is_empty() {
            return;
        }
        self.leader.card_id = id.clone();
        self.note_card(&id);
        self.sync_leader_fields();
    }

    pub fn sync_leader_fields(&mut self) {
        self.leader_id = self.leader.card_id.clone();
        self.leader_power = self.leader.effective_power();
        self.leader_rested = self.leader.rested;
    }

    pub fn board(&self) -> &[CardInstance] {
        &self.characters
    }

    pub fn active_don(&self) -> u32 {
        self.don_active
    }

    pub fn rested_don(&self) -> u32 {
        self.don_rested
    }

    pub fn total_don(&self) -> u32 {
        self.don_active + self.don_rested
    }

    pub fn find_character(&mut self, card_id: &str) -> Option<&mut CardInstance> {
        self.characters.iter_mut().find(|c| c.card_id == card_id)
    }

    pub fn find_character_by_instance(&mut self, instance_id: &Uuid) -> Option<&mut CardInstance> {
        self.characters
            .iter_mut()
            .find(|c| c.instance_id == *instance_id)
    }

    pub fn find_blocker(&self) -> Option<&CardInstance> {
        self.characters.iter().find(|c| {
            c.state == CardState::Ready && c.zone == Zone::Character && !c.tapped && !c.rested
        })
    }

    pub fn push_trash(&mut self, card: CardInstance) {
        self.trash.push(card);
        self.trash_count = self.trash.len() as u32;
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
    pub status: ConnectionStatus,
    pub websocket_connected: bool,
    pub file_monitor_active: bool,
    pub last_event_at: Option<DateTime<Utc>>,
    pub events_processed: u64,
    pub latency_ms: u64,
    pub last_error: Option<String>,
}

/// Backend connectivity model for HUD display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// Top-level canonical game state — single source of truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    #[serde(alias = "match_id")]
    pub game_id: Uuid,
    pub turn_number: u32,
    pub active_player: u8,
    pub phase: Phase,
    pub players: [PlayerState; 2],
    pub combat: CombatState,
    pub connection: ConnectionState,
    pub event_sequence: u64,
    pub last_event: Option<LastEventInfo>,
    pub event_log: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub last_processed_fingerprint: Option<String>,
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

impl GameState {
    pub fn new() -> Self {
        Self {
            game_id: Uuid::new_v4(),
            active_player: 0,
            turn_number: 1,
            phase: Phase::Draw,
            players: [
                PlayerState::new(0, "ST01-001"),
                PlayerState::new(1, "ST01-001"),
            ],
            combat: CombatState::default(),
            connection: ConnectionState::default(),
            event_sequence: 0,
            last_event: None,
            event_log: Vec::new(),
            timestamp: Utc::now(),
            last_processed_fingerprint: None,
        }
    }

    pub fn player_one(&self) -> &PlayerState {
        &self.players[0]
    }

    pub fn player_two(&self) -> &PlayerState {
        &self.players[1]
    }

    pub fn player_one_mut(&mut self) -> &mut PlayerState {
        &mut self.players[0]
    }

    pub fn player_two_mut(&mut self) -> &mut PlayerState {
        &mut self.players[1]
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
