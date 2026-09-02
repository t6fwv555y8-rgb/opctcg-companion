use chrono::{DateTime, Utc};
use optcg_core::GameState;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type GameSessionId = Uuid;

/// Active observation session bound to one authoritative source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSession {
    pub id: GameSessionId,
    pub source: crate::types::ObservationSource,
    pub started_at: DateTime<Utc>,
    pub state: GameState,
    pub observation_sequence: u64,
    pub event_sequence: u64,
    pub confidence: f32,
}

impl GameSession {
    pub fn new(source: crate::types::ObservationSource) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            started_at: Utc::now(),
            state: GameState::new(),
            observation_sequence: 0,
            event_sequence: 0,
            confidence: 1.0,
        }
    }

    pub fn reset_for_source(&mut self, source: crate::types::ObservationSource) {
        *self = Self::new(source);
    }
}
