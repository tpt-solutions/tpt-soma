//! Oncology pathology models (Phase 4, `tpt-soma-pathos`).
//!
//! These are transparent, illustrative proxies built from interpretable inputs
//! (immune-fraction proxies, tumor mutational burden proxy). They are research
//! aids, not validated clinical classifiers, and are clearly labeled as such.

use crate::{PathosError, Result};

/// Tumor-microenvironment (TME) immunosuppression proxy in [0, 1].
///
/// Higher M2-like macrophage fraction and lower cytotoxic CD8+ T-cell fraction
/// indicate a more immunosuppressive, immune-cold TME.
pub fn tme_immunosuppression_score(m2_macrophage_frac: f64, cd8_tcell_frac: f64) -> Result<f64> {
    if !(0.0..=1.0).contains(&m2_macrophage_frac) || !(0.0..=1.0).contains(&cd8_tcell_frac) {
        return Err(PathosError::InvalidInput(
            "cell fractions must be in [0, 1]".to_string(),
        ));
    }
    let score = 0.7 * m2_macrophage_frac - 0.3 * cd8_tcell_frac;
    Ok(score.clamp(0.0, 1.0))
}

/// Binary immunotherapy-response proxy.
///
/// Favorable when the TME is not immunosuppressive (`tme_score < 0.4`) and the
/// tumor mutational burden proxy is high (`tmb_proxy > 10`).
pub fn immunotherapy_response_proxy(tme_score: f64, tmb_proxy: f64) -> Result<bool> {
    if tmb_proxy < 0.0 {
        return Err(PathosError::InvalidInput(
            "tmb_proxy must be non-negative".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&tme_score) {
        return Err(PathosError::InvalidInput(
            "tme_score must be in [0, 1]".to_string(),
        ));
    }
    Ok(tme_score < 0.4 && tmb_proxy > 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tme_score_bounds_and_direction() {
        // 0.7*0.8 - 0.3*0.05 = 0.56 - 0.015 = 0.545
        let cold = tme_immunosuppression_score(0.8, 0.05).unwrap();
        // 0.7*0.1 - 0.3*0.6 = 0.07 - 0.18 = -0.11 -> clamped to 0.0
        let hot = tme_immunosuppression_score(0.1, 0.6).unwrap();
        assert!(cold > hot);
        assert!((cold - 0.545).abs() < 1e-9);
        assert_eq!(hot, 0.0);
    }

    #[test]
    fn test_tme_score_rejects_invalid() {
        assert!(tme_immunosuppression_score(1.5, 0.0).is_err());
    }

    #[test]
    fn test_immunotherapy_response_proxy() {
        assert!(immunotherapy_response_proxy(0.2, 15.0).unwrap());
        assert!(!immunotherapy_response_proxy(0.7, 15.0).unwrap());
        assert!(!immunotherapy_response_proxy(0.2, 5.0).unwrap());
    }
}
