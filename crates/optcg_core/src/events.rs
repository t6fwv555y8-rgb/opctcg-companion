use crate::types::Phase;
use serde::{Deserialize, Serialize};

/// Strongly typed player identifier (0 = player one, 1 = player two).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlayerId {
    Player1 = 0,
    Player2 = 1,
}

impl PlayerId {
    pub fn from_index(index: u8) -> Result<Self, crate::error::CoreError> {
        match index {
            0 => Ok(PlayerId::Player1),
            1 => Ok(PlayerId::Player2),
            n => Err(crate::error::CoreError::PlayerOutOfBounds(n as usize)),
        }
    }

    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn opponent(self) -> Self {
        match self {
            PlayerId::Player1 => PlayerId::Player2,
            PlayerId::Player2 => PlayerId::Player1,
        }
    }

    pub fn parse_token(s: &str) -> Result<Self, crate::error::CoreError> {
        match s.to_ascii_uppercase().as_str() {
            "PLAYER_1" | "PLAYER1" | "P1" | "0" => Ok(PlayerId::Player1),
            "PLAYER_2" | "PLAYER2" | "P2" | "1" => Ok(PlayerId::Player2),
            other => Err(crate::error::CoreError::InvalidPayload(format!(
                "invalid player token: {other}"
            ))),
        }
    }
}

/// Attack target specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttackTarget {
    Leader { player: PlayerId },
    Character { player: PlayerId, card_id: String },
}

impl AttackTarget {
    pub fn target_player(&self) -> PlayerId {
        match self {
            AttackTarget::Leader { player } | AttackTarget::Character { player, .. } => *player,
        }
    }
}

/// Canonical internal game events — deterministic and testable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GameEvent {
    GameStarted,
    PhaseChanged {
        phase: Phase,
    },
    TurnStarted {
        player: PlayerId,
    },
    TurnEnded {
        next_player: PlayerId,
    },
    CardPlayed {
        player: PlayerId,
        card_id: String,
        zone: Option<String>,
    },
    DonAttached {
        player: PlayerId,
        target: String,
        amount: u8,
    },
    AttackDeclared {
        attacker: String,
        attacker_player: PlayerId,
        target: AttackTarget,
        power: u32,
    },
    BlockerActivated {
        player: PlayerId,
        card_instance: String,
    },
    BlockerOffered {
        player: PlayerId,
        blocker_id: String,
    },
    CombatResolved {
        damage: Option<u32>,
        blocked: Option<bool>,
    },
    LifeChanged {
        player: PlayerId,
        delta: i8,
    },
    CardAddedToHand {
        player: PlayerId,
        card_id: String,
    },
    CardRemovedFromBoard {
        player: PlayerId,
        card_instance: String,
    },
    DrawCard {
        player: PlayerId,
        count: u8,
    },
    VisionPhaseHint {
        phase: Phase,
    },
    StateSync {
        payload: serde_json::Value,
    },
}

impl GameEvent {
    pub fn name(&self) -> &'static str {
        match self {
            GameEvent::GameStarted => "GAME_STARTED",
            GameEvent::PhaseChanged { .. } => "PHASE_CHANGED",
            GameEvent::TurnStarted { .. } => "TURN_STARTED",
            GameEvent::TurnEnded { .. } => "TURN_ENDED",
            GameEvent::CardPlayed { .. } => "CARD_PLAYED",
            GameEvent::DonAttached { .. } => "DON_ATTACHED",
            GameEvent::AttackDeclared { .. } => "ATTACK_DECLARED",
            GameEvent::BlockerActivated { .. } => "BLOCKER_ACTIVATED",
            GameEvent::BlockerOffered { .. } => "BLOCKER_OFFERED",
            GameEvent::CombatResolved { .. } => "COMBAT_RESOLVED",
            GameEvent::LifeChanged { .. } => "LIFE_CHANGED",
            GameEvent::CardAddedToHand { .. } => "CARD_ADDED_TO_HAND",
            GameEvent::CardRemovedFromBoard { .. } => "CARD_REMOVED_FROM_BOARD",
            GameEvent::DrawCard { .. } => "DRAW_CARD",
            GameEvent::VisionPhaseHint { .. } => "VISION_PHASE_HINT",
            GameEvent::StateSync { .. } => "STATE_SYNC",
        }
    }
}

/// Metadata for the last successfully processed event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastEventInfo {
    pub sequence: u64,
    pub event_name: String,
    pub summary: String,
    pub processed_at: chrono::DateTime<chrono::Utc>,
}
