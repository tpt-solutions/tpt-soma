//! Longevity & aging models (Phase 4, `tpt-soma-pathos`).
//!
//! Illustrative proxies built from interpretable cardiometabolic markers.
//! Not validated epigenetic clocks; clearly labeled as research aids.

use crate::{PathosError, Result};

/// Phenotypic-age proxy (illustrative).
///
/// Chronological age plus weighted deviations of cardiometabolic markers from
/// healthy baselines (systolic BP, BMI, fasting glucose).
pub fn phenotypic_age_proxy(
    chronological_age: f64,
    systolic_bp: f64,
    bmi: f64,
    fasting_glucose: f64,
) -> Result<f64> {
    if chronological_age <= 0.0 {
        return Err(PathosError::InvalidInput(
            "chronological_age must be positive".to_string(),
        ));
    }
    let bp_dev = ((systolic_bp - 120.0) / 120.0).max(0.0) * 5.0;
    let bmi_dev = ((bmi - 25.0).abs() / 25.0) * 3.0;
    let glu_dev = ((fasting_glucose - 90.0).max(0.0) / 90.0) * 4.0;
    Ok(chronological_age + bp_dev + bmi_dev + glu_dev)
}

/// Senescence index: excess phenotypic over chronological age (>= 0).
pub fn senescence_index(chronological_age: f64, phenotypic_age: f64) -> Result<f64> {
    if chronological_age <= 0.0 {
        return Err(PathosError::InvalidInput(
            "chronological_age must be positive".to_string(),
        ));
    }
    Ok((phenotypic_age - chronological_age).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phenotypic_age_exceeds_chronological_under_stress() {
        let pa = phenotypic_age_proxy(40.0, 160.0, 30.0, 120.0).unwrap();
        assert!(pa > 40.0);
    }

    #[test]
    fn test_phenotypic_age_rejects_nonpositive() {
        assert!(phenotypic_age_proxy(0.0, 120.0, 25.0, 90.0).is_err());
    }

    #[test]
    fn test_senescence_index_nonnegative() {
        let si = senescence_index(40.0, 35.0).unwrap();
        assert_eq!(si, 0.0);
        let si2 = senescence_index(40.0, 50.0).unwrap();
        assert_eq!(si2, 10.0);
    }
}
