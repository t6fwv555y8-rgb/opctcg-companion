use thiserror::Error;

#[derive(Debug, Error)]
pub enum RulesError {
    #[error("database error: {0}")]
    Database(#[from] optcg_database::DatabaseError),

    #[error("invalid action: {0}")]
    InvalidAction(String),

    #[error("simulation depth exceeded")]
    DepthExceeded,
}

pub type RulesResult<T> = Result<T, RulesError>;
