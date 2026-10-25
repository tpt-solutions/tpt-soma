//! Autoimmune disease-activity models (Phase 4, `tpt-soma-pathos`).
//!
//! Implements the published DAS28-CRP disease-activity score for rheumatoid
//! arthritis (Prevoo et al., 1995) and its standard EULAR activity bands.
//! The coefficients are fixed by the published formula; verify against the
//! source before any clinical use.

use crate::{PathosError, Result};

/// Joint and laboratory inputs for DAS28-CRP.
#[derive(Debug, Clone, Copy)]
pub struct Das28Input {
    /// Tender joint count, 28-joint set (0..=28).
    pub tender_joints: u8,
    /// Swollen joint count, 28-joint set (0..=28).
    pub swollen_joints: u8,
    /// C-reactive protein in mg/L (>= 0).
    pub crp_mg_l: f64,
    /// Patient global health on a 0–100 visual analogue scale.
    pub patient_global_health: f64,
}

/// EULAR disease-activity category derived from DAS28.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiseaseActivity {
    Remission,
    Low,
    Moderate,
    High,
}

/// DAS28-CRP: `0.56√TJC + 0.28√SJC + 0.36·ln(CRP + 1) + 0.014·GH + 0.96`.
pub fn das28_crp(input: &Das28Input) -> Result<f64> {
    if input.tender_joints > 28 || input.swollen_joints > 28 {
        return Err(PathosError::InvalidInput(
            "joint counts must be in 0..=28".to_string(),
        ));
    }
    if input.crp_mg_l < 0.0 {
        return Err(PathosError::InvalidInput(
            "crp_mg_l must be non-negative".to_string(),
        ));
    }
    if !(0.0..=100.0).contains(&input.patient_global_health) {
        return Err(PathosError::InvalidInput(
            "patient_global_health must be in [0, 100]".to_string(),
        ));
    }
    let tjc = f64::from(input.tender_joints);
    let sjc = f64::from(input.swollen_joints);
    Ok(0.56 * tjc.sqrt()
        + 0.28 * sjc.sqrt()
        + 0.36 * (input.crp_mg_l + 1.0).ln()
        + 0.014 * input.patient_global_health
        + 0.96)
}

/// EULAR DAS28 bands: remission < 2.6, low ≤ 3.2, moderate ≤ 5.1, high > 5.1.
pub fn classify_das28(score: f64) -> Result<DiseaseActivity> {
    if !score.is_finite() || score < 0.0 {
        return Err(PathosError::InvalidInput(
            "DAS28 score must be a non-negative finite number".to_string(),
        ));
    }
    Ok(if score < 2.6 {
        DiseaseActivity::Remission
    } else if score <= 3.2 {
        DiseaseActivity::Low
    } else if score <= 5.1 {
        DiseaseActivity::Moderate
    } else {
        DiseaseActivity::High
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(tjc: u8, sjc: u8, crp: f64, gh: f64) -> Das28Input {
        Das28Input {
            tender_joints: tjc,
            swollen_joints: sjc,
            crp_mg_l: crp,
            patient_global_health: gh,
        }
    }

    #[test]
    fn test_das28_crp_worked_example() {
        // 0.56*2 + 0.28*sqrt(2) + 0.36*ln(11) + 0.014*50 + 0.96 ≈ 4.0392
        let score = das28_crp(&input(4, 2, 10.0, 50.0)).unwrap();
        assert!((score - 4.0392).abs() < 1e-3, "got {score}");
    }

    #[test]
    fn test_das28_crp_increases_with_each_component() {
        let base = das28_crp(&input(4, 2, 10.0, 50.0)).unwrap();
        assert!(das28_crp(&input(8, 2, 10.0, 50.0)).unwrap() > base);
        assert!(das28_crp(&input(4, 6, 10.0, 50.0)).unwrap() > base);
        assert!(das28_crp(&input(4, 2, 40.0, 50.0)).unwrap() > base);
        assert!(das28_crp(&input(4, 2, 10.0, 80.0)).unwrap() > base);
    }

    #[test]
    fn test_das28_crp_rejects_invalid_inputs() {
        assert!(das28_crp(&input(29, 0, 1.0, 10.0)).is_err());
        assert!(das28_crp(&input(0, 0, -1.0, 10.0)).is_err());
        assert!(das28_crp(&input(0, 0, 1.0, 101.0)).is_err());
    }

    #[test]
    fn test_classify_das28_bands() {
        assert_eq!(classify_das28(2.0).unwrap(), DiseaseActivity::Remission);
        assert_eq!(classify_das28(3.0).unwrap(), DiseaseActivity::Low);
        assert_eq!(classify_das28(4.0).unwrap(), DiseaseActivity::Moderate);
        assert_eq!(classify_das28(5.5).unwrap(), DiseaseActivity::High);
        assert!(classify_das28(-1.0).is_err());
        assert!(classify_das28(f64::NAN).is_err());
    }
}
