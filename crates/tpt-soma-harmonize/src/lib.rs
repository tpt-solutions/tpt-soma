pub mod io;
pub mod mapping;
pub mod review;

pub use io::{export_queue_to_csv, import_csv_mappings};
pub use mapping::{MappingEntry, MappingTable, OntologySource};
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
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
}

pub type Error = HarmonizeError;
pub type Result<T> = std::result::Result<T, Error>;
