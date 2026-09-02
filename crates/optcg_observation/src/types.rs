use optcg_core::{AttackTarget, Phase, PlayerId, Zone};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Where an observation originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSource {
    Mock,
    DesktopSimulator,
    BrowserSimulator,
    ScreenVision,
    Replay,
}

impl ObservationSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Mock => "Mock Game",
            Self::DesktopSimulator => "OPTCGSim",
            Self::BrowserSimulator => "OneSimulator",
            Self::ScreenVision => "Screen Vision",
            Self::Replay => "Replay",
        }
    }
}

/// Strongly typed card instance reference for observations.
pub type CardInstanceId = Uuid;

/// Board position hint from an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BoardPosition {
    pub slot: u8,
    pub row: u8,
}

/// Simulator-independent observation — not treated as ground truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObservationEvent {
    GameDetected {
        source: ObservationSource,
        confidence: f32,
    },
    PhaseObserved {
        phase: Phase,
        confidence: f32,
    },
    TurnObserved {
        player: PlayerId,
        confidence: f32,
    },
    CardObserved {
        card_id: Option<String>,
        owner: PlayerId,
        zone: Zone,
        position: Option<BoardPosition>,
        confidence: f32,
    },
    CardMoved {
        instance_id: Option<CardInstanceId>,
        card_id: Option<String>,
        from: Zone,
        to: Zone,
        confidence: f32,
    },
    DonObserved {
        player: PlayerId,
        active: u8,
        rested: u8,
        attached: u8,
        confidence: f32,
    },
    LifeObserved {
        player: PlayerId,
        count: u8,
        confidence: f32,
    },
    AttackObserved {
        attacker: Option<CardInstanceId>,
        attacker_card_id: Option<String>,
        target: Option<AttackTarget>,
        observed_power: Option<u32>,
        confidence: f32,
    },
    HandCountObserved {
        player: PlayerId,
        count: usize,
        confidence: f32,
    },
    /// High-confidence structured raw event (mock/logs) — reconciler parses to GameEvent.
    StructuredRaw {
        raw: String,
        source: ObservationSource,
        confidence: f32,
    },
}

impl ObservationEvent {
    pub fn confidence(&self) -> f32 {
        match self {
            Self::GameDetected { confidence, .. }
            | Self::PhaseObserved { confidence, .. }
            | Self::TurnObserved { confidence, .. }
            | Self::CardObserved { confidence, .. }
            | Self::CardMoved { confidence, .. }
            | Self::DonObserved { confidence, .. }
            | Self::LifeObserved { confidence, .. }
            | Self::AttackObserved { confidence, .. }
            | Self::HandCountObserved { confidence, .. }
            | Self::StructuredRaw { confidence, .. } => *confidence,
        }
    }

    pub fn source_hint(&self) -> Option<ObservationSource> {
        match self {
            Self::GameDetected { source, .. } | Self::StructuredRaw { source, .. } => Some(*source),
            _ => None,
        }
    }
}

/// Envelope for transport with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationEnvelope {
    pub sequence: u64,
    pub timestamp_ms: i64,
    pub source: ObservationSource,
    pub event: ObservationEvent,
}
