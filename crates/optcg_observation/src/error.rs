use thiserror::Error;

#[derive(Debug, Error)]
pub enum ObservationError {
    #[error("adapter error: {0}")]
    Adapter(String),

    #[error("reconcile error: {0}")]
    Reconcile(String),

    #[error("core error: {0}")]
    Core(#[from] optcg_core::CoreError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("payload too large: {0} bytes")]
    PayloadTooLarge(usize),

    #[error("adapter unavailable: {0}")]
    Unavailable(String),
}

pub type ObsResult<T> = Result<T, ObservationError>;
