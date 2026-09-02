use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("unknown event type: {0}")]
    UnknownEvent(String),

    #[error("invalid event payload: {0}")]
    InvalidPayload(String),

    #[error("player index out of bounds: {0}")]
    PlayerOutOfBounds(usize),

    #[error("invalid phase: {0}")]
    InvalidPhase(String),

    #[error("card not found: {0}")]
    CardNotFound(String),

    #[error("duplicate event: {0}")]
    DuplicateEvent(String),

    #[error("out of order event: expected > {expected}, got {got}")]
    OutOfOrderEvent { expected: u64, got: u64 },

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type CoreResult<T> = Result<T, CoreError>;
