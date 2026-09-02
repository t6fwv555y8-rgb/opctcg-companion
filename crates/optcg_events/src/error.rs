use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventsError {
    #[error("notify error: {0}")]
    Notify(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("websocket error: {0}")]
    WebSocket(String),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("core error: {0}")]
    Core(#[from] optcg_core::CoreError),
}

pub type EventsResult<T> = Result<T, EventsError>;
