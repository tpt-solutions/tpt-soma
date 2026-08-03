//! tpt-soma-organon: Organ function and physiological domain module
//!
//! This crate handles organ function tests, organ imaging, and cross-organ
//! coupling analysis. It builds on the Phase 1 genomic/cytos foundation
//! and integrates with Phase 2's chronos time-series engine.

pub mod calculator;
pub mod imaging;
pub mod ingestion;
pub mod organ_system;
pub mod storage;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrganonError {
    #[error("calculation error: {0}")]
    Calculation(String),
    #[error("ingestion error: {0}")]
    Ingestion(String),
    #[error("imaging error: {0}")]
    Imaging(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("harmonization error: {0}")]
    Harmonization(#[from] tpt_soma_harmonize::HarmonizeError),
    #[error("capability error: {0}")]
    Capability(String),
    #[error("audit error: {0}")]
    Audit(String),
    #[error("core error: {0}")]
    Core(#[from] tpt_soma_core::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("chronos error: {0}")]
    Chronos(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl From<Box<dyn std::error::Error + Send + Sync>> for OrganonError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        let err_str = err.to_string();
        if err_str.contains("capability")
            || err_str.contains("signing")
            || err_str.contains("token")
        {
            OrganonError::Capability(err_str)
        } else if err_str.contains("audit")
            || err_str.contains("ledger")
            || err_str.contains("integrity")
        {
            OrganonError::Audit(err_str)
        } else if err_str.contains("chronos")
            || err_str.contains("resampling")
            || err_str.contains("variability")
            || err_str.contains("trajectory")
        {
            OrganonError::Chronos(err_str)
        } else {
            OrganonError::Capability(err_str)
        }
    }
}

pub type Result<T> = std::result::Result<T, OrganonError>;

/// Data classes introduced in Phase 2 for organon
pub const DATA_CLASS_CLINICAL_OBSERVATION: &str = "clinical_observation";
pub const DATA_CLASS_ORGAN_IMAGING: &str = "organ_imaging";

/// Organ system identifiers (UBERON-based)
pub mod organ_systems {
    pub const CARDIOVASCULAR: &str = "UBERON:0004535";
    pub const RESPIRATORY: &str = "UBERON:0001004";
    pub const RENAL: &str = "UBERON:0002113";
    pub const HEPATIC: &str = "UBERON:0002107";
    pub const ENDOCRINE: &str = "UBERON:0000949";
    pub const NERVOUS: &str = "UBERON:0001016";
    pub const GASTROINTESTINAL: &str = "UBERON:0001007";
    pub const MUSCULOSKELETAL: &str = "UBERON:0002204";
    pub const IMMUNE: &str = "UBERON:0002405";
}

#[cfg(test)]
mod tests {
    use super::OrganonError;

    #[test]
    fn test_organon_error_display() {
        let err = OrganonError::Calculation("test".to_string());
        assert!(err.to_string().contains("calculation error"));
    }
}
