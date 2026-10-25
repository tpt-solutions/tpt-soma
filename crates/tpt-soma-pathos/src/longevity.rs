//! Longevity & aging models (Phase 4, `tpt-soma-pathos`).
//!
//! Illustrative proxies built from interpretable cardiometabolic markers.
//! Not validated epigenetic clocks; clearly labeled as research aids.

use crate::{PathosError, Result};

/// Gompertz mortality model: `h(x) = a * exp(b * x)` for age `x`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GompertzHazard {
    /// Baseline hazard `a` at age zero.
    pub baseline_hazard: f64,
    /// Exponential ageing rate `b` (per year of age).
    pub rate: f64,
}

impl GompertzHazard {
    pub fn hazard(&self, age: f64) -> f64 {
        self.baseline_hazard * (self.rate * age).exp()
    }

    /// Years for the mortality hazard to double: `ln 2 / b`.
    pub fn doubling_time(&self) -> f64 {
        std::f64::consts::LN_2 / self.rate
    }
}

/// Fit a Gompertz hazard to observed `(age, hazard)` pairs.
///
/// Log-linear least squares: `ln h = ln a + b x`. Requires at least two distinct
/// ages and strictly positive hazards.
pub fn fit_gompertz_hazard(ages: &[f64], hazards: &[f64]) -> Result<GompertzHazard> {
    if ages.len() != hazards.len() || ages.len() < 2 {
        return Err(PathosError::InvalidInput(
            "need at least two (age, hazard) pairs of equal length".to_string(),
        ));
    }
    if hazards.iter().any(|h| *h <= 0.0) {
        return Err(PathosError::InvalidInput(
            "hazards must be strictly positive".to_string(),
        ));
    }
    let n = ages.len() as f64;
    let mean_x = ages.iter().sum::<f64>() / n;
    let logs: Vec<f64> = hazards.iter().map(|h| h.ln()).collect();
    let mean_y = logs.iter().sum::<f64>() / n;
    let sxx: f64 = ages.iter().map(|x| (x - mean_x).powi(2)).sum();
    if sxx == 0.0 {
        return Err(PathosError::InvalidInput(
            "ages must not all be equal".to_string(),
        ));
    }
    let sxy: f64 = ages
        .iter()
        .zip(&logs)
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum();
    let rate = sxy / sxx;
    let intercept = mean_y - rate * mean_x;
    Ok(GompertzHazard {
        baseline_hazard: intercept.exp(),
        rate,
    })
}

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
    fn test_gompertz_fit_recovers_known_parameters() {
        let truth = GompertzHazard {
            baseline_hazard: 0.001,
            rate: 0.085,
        };
        let ages: Vec<f64> = (30..=90).step_by(10).map(f64::from).collect();
        let hazards: Vec<f64> = ages.iter().map(|x| truth.hazard(*x)).collect();
        let fit = fit_gompertz_hazard(&ages, &hazards).unwrap();
        assert!((fit.rate - truth.rate).abs() < 1e-9, "rate {}", fit.rate);
        assert!(
            (fit.baseline_hazard / truth.baseline_hazard - 1.0).abs() < 1e-9,
            "baseline {}",
            fit.baseline_hazard
        );
        // Mortality roughly doubles every ~8 years at this rate.
        assert!((fit.doubling_time() - 8.15).abs() < 0.01);
    }

    #[test]
    fn test_gompertz_fit_rejects_degenerate_input() {
        assert!(fit_gompertz_hazard(&[50.0], &[0.01]).is_err());
        assert!(fit_gompertz_hazard(&[50.0, 50.0], &[0.01, 0.02]).is_err());
        assert!(fit_gompertz_hazard(&[50.0, 60.0], &[0.01, 0.0]).is_err());
    }

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
