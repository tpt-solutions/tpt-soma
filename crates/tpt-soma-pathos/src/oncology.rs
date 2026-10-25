//! Oncology pathology models (Phase 4, `tpt-soma-pathos`).
//!
//! These are transparent, illustrative proxies built from interpretable inputs
//! (immune-fraction proxies, tumor mutational burden proxy). They are research
//! aids, not validated clinical classifiers, and are clearly labeled as such.

use crate::{PathosError, Result};
use tpt_soma_simulacrum::solver::{OdeSystem, integrate};

/// Gompertz tumor growth: `dN/dt = a N ln(K / N)`, saturating at capacity `K`.
#[derive(Debug, Clone, Copy)]
pub struct Gompertz {
    /// Growth-rate parameter `a` (per time unit).
    pub rate: f64,
    /// Carrying capacity `K` (same units as tumor size).
    pub carrying_capacity: f64,
}

impl OdeSystem for Gompertz {
    fn dim(&self) -> usize {
        1
    }

    fn deriv(&self, _t: f64, y: &[f64], dydt: &mut [f64]) {
        let n = y[0].max(f64::MIN_POSITIVE);
        dydt[0] = self.rate * n * (self.carrying_capacity / n).ln();
    }
}

/// Closed-form Gompertz solution: `N(t) = K exp(ln(N0 / K) e^{-a t})`.
pub fn gompertz_closed_form(n0: f64, carrying_capacity: f64, rate: f64, t: f64) -> f64 {
    carrying_capacity * ((n0 / carrying_capacity).ln() * (-rate * t).exp()).exp()
}

/// Simulate tumor size `(t, N)` under Gompertz growth from `N(0) = n0`.
pub fn simulate_gompertz_growth(
    n0: f64,
    carrying_capacity: f64,
    rate: f64,
    t_end: f64,
    dt: f64,
) -> Result<Vec<(f64, f64)>> {
    if n0 <= 0.0 || carrying_capacity <= 0.0 || rate <= 0.0 {
        return Err(PathosError::InvalidInput(
            "n0, carrying_capacity, and rate must be positive".to_string(),
        ));
    }
    if t_end <= 0.0 || dt <= 0.0 {
        return Err(PathosError::InvalidInput(
            "t_end and dt must be positive".to_string(),
        ));
    }
    let steps = (t_end / dt).round() as usize;
    let sys = Gompertz {
        rate,
        carrying_capacity,
    };
    Ok(integrate(&sys, 0.0, &[n0], dt, steps)
        .into_iter()
        .map(|(t, y)| (t, y[0]))
        .collect())
}

/// RECIST 1.1 target-lesion response category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecistResponse {
    CompleteResponse,
    PartialResponse,
    StableDisease,
    ProgressiveDisease,
}

/// RECIST 1.1 response from the sum of target-lesion longest diameters (mm).
///
/// - Progressive disease: at least 20% and at least 5 mm above the nadir.
/// - Complete response: all target lesions gone (sum is zero).
/// - Partial response: at least 30% decrease from baseline.
/// - Otherwise stable disease.
///
/// Progression is checked first, as RECIST requires. Non-target lesions and
/// new-lesion rules are outside this function's inputs.
pub fn recist_response(
    baseline_sum_mm: f64,
    nadir_sum_mm: f64,
    current_sum_mm: f64,
) -> Result<RecistResponse> {
    if baseline_sum_mm <= 0.0 {
        return Err(PathosError::InvalidInput(
            "baseline_sum_mm must be positive".to_string(),
        ));
    }
    if !(0.0..=baseline_sum_mm).contains(&nadir_sum_mm) || current_sum_mm < 0.0 {
        return Err(PathosError::InvalidInput(
            "require 0 <= nadir_sum_mm <= baseline_sum_mm and current_sum_mm >= 0".to_string(),
        ));
    }
    let increase = current_sum_mm - nadir_sum_mm;
    if increase >= 5.0 && increase >= 0.2 * nadir_sum_mm {
        return Ok(RecistResponse::ProgressiveDisease);
    }
    if current_sum_mm == 0.0 {
        return Ok(RecistResponse::CompleteResponse);
    }
    let decrease = (baseline_sum_mm - current_sum_mm) / baseline_sum_mm;
    if decrease >= 0.30 {
        return Ok(RecistResponse::PartialResponse);
    }
    Ok(RecistResponse::StableDisease)
}

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
    fn test_gompertz_simulation_matches_closed_form() {
        let traj = simulate_gompertz_growth(1.0, 100.0, 0.2, 30.0, 0.001).unwrap();
        let (t, n) = *traj.last().unwrap();
        let analytic = gompertz_closed_form(1.0, 100.0, 0.2, t);
        assert!((n - analytic).abs() / analytic < 1e-6, "{n} vs {analytic}");
    }

    #[test]
    fn test_gompertz_saturates_at_carrying_capacity() {
        let traj = simulate_gompertz_growth(1.0, 100.0, 0.5, 60.0, 0.01).unwrap();
        let (_, n) = *traj.last().unwrap();
        assert!((n - 100.0).abs() < 0.1, "final size {n}");
    }

    #[test]
    fn test_recist_response_categories() {
        use RecistResponse::*;
        // 40% shrinkage from baseline.
        assert_eq!(
            recist_response(100.0, 100.0, 60.0).unwrap(),
            PartialResponse
        );
        // Exactly 30% shrinkage still qualifies as partial response.
        assert_eq!(
            recist_response(100.0, 100.0, 70.0).unwrap(),
            PartialResponse
        );
        // 25% shrinkage is stable disease.
        assert_eq!(recist_response(100.0, 100.0, 75.0).unwrap(), StableDisease);
        // 50% above nadir and >= 5 mm absolute is progression.
        assert_eq!(
            recist_response(100.0, 60.0, 90.0).unwrap(),
            ProgressiveDisease
        );
        // 5 mm above nadir but only ~5.6% of it: not progression, and only 5%
        // below baseline, so stable disease.
        assert_eq!(recist_response(100.0, 90.0, 95.0).unwrap(), StableDisease);
        // 20% above nadir but under 5 mm absolute: not progression.
        assert_eq!(recist_response(100.0, 20.0, 24.0).unwrap(), PartialResponse);
        // 8% above nadir with 5 mm absolute is not progression (needs 20%); it is
        // still 35% below baseline, so partial response.
        assert_eq!(recist_response(100.0, 60.0, 65.0).unwrap(), PartialResponse);
        // All target lesions gone.
        assert_eq!(recist_response(100.0, 80.0, 0.0).unwrap(), CompleteResponse);
    }

    #[test]
    fn test_recist_rejects_invalid_sums() {
        assert!(recist_response(0.0, 0.0, 0.0).is_err());
        assert!(recist_response(100.0, 120.0, 90.0).is_err());
        assert!(recist_response(100.0, 90.0, -1.0).is_err());
    }

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
