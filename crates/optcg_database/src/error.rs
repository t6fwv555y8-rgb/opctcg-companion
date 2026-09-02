use thiserror::Error;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("card not found: {0}")]
    CardNotFound(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type DbResult<T> = Result<T, DatabaseError>;
