pub mod mapping;
pub mod review;

pub use mapping::MappingTable;
pub use review::{ReviewQueue, Unmapped};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HarmonizeError {
    #[error("mapping error: {0}")]
    Mapping(String),
    #[error("review error: {0}")]
    Review(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Error = HarmonizeError;
pub type Result<T> = std::result::Result<T, Error>;
