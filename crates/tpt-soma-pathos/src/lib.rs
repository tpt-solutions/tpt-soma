//! tpt-soma-pathos: Computational pathology (Phase 4).
//!
//! Initial slice: metabolic & endocrine (diabetes) modeling built on Phase 2's
//! CGM / clinical-observation data — insulin-resistance estimation (HOMA-IR),
//! metabolic syndrome classification (NCEP ATP III), and a combined
//! insulin-resistance risk score. Later phases extend this with oncology,
//! longevity, cardiovascular, autoimmune, and infectious sub-modules.

pub mod error;
pub mod metabolic;

pub use error::{PathosError, Result};

/// Data class for computational pathology findings.
pub const DATA_CLASS_PATHOS_FINDING: &str = "pathos_finding";
/// Data class for clinical-trial design / cohort-discovery metadata.
pub const DATA_CLASS_CLINICAL_TRIAL: &str = "clinical_trial";
/// Data class for biomarker discovery/validation outputs.
pub const DATA_CLASS_BIOMARKER: &str = "biomarker_discovery";

#[cfg(test)]
mod tests {
    #[test]
    fn test_data_class_constants() {
        assert_eq!(crate::DATA_CLASS_PATHOS_FINDING, "pathos_finding");
        assert_eq!(crate::DATA_CLASS_CLINICAL_TRIAL, "clinical_trial");
        assert_eq!(crate::DATA_CLASS_BIOMARKER, "biomarker_discovery");
    }
}
