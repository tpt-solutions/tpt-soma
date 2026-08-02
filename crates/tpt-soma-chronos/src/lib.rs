//! tpt-soma-chronos: Time-series and longitudinal data engine
//!
//! This crate handles continuous sensor data (CGM), longitudinal organ function
//! trajectories, gap-filling/resampling, and glycemic variability calculations.

pub mod cgm;
pub mod resampling;
pub mod trajectory;
pub mod variability;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChronosError {
    #[error("resampling error: {0}")]
    Resampling(String),
    #[error("gap filling error: {0}")]
    GapFilling(String),
    #[error("variability calculation error: {0}")]
    Variability(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
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
}

impl From<Box<dyn std::error::Error + Send + Sync>> for ChronosError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        let err_str = err.to_string();
        if err_str.contains("capability") || err_str.contains("signing") || err_str.contains("token") {
            ChronosError::Capability(err_str)
        } else if err_str.contains("audit") || err_str.contains("ledger") || err_str.contains("integrity") {
            ChronosError::Audit(err_str)
        } else {
            ChronosError::Capability(err_str)
        }
    }
}

pub type Result<T> = std::result::Result<T, ChronosError>;

/// Data classes introduced in Phase 2 for chronos
pub const DATA_CLASS_CGM_CONTINUOUS: &str = "cgm_continuous";
pub const DATA_CLASS_ORGAN_FUNCTION_TRAJECTORY: &str = "organ_function_trajectory";

#[cfg(test)]
mod tests {
    use super::ChronosError;

    #[test]
    fn test_chronos_error_display() {
        let err = ChronosError::Resampling("test".to_string());
        assert!(err.to_string().contains("resampling error"));
    }
}
