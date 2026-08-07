use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimulacrumError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("solver error: {0}")]
    Solver(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("core error: {0}")]
    Core(#[from] tpt_soma_core::Error),
}

pub type Result<T> = std::result::Result<T, SimulacrumError>;
