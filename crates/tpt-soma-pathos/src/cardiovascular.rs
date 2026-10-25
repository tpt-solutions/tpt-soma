//! Cardiovascular models (Phase 4, `tpt-soma-pathos`).
//!
//! A simplified ASCVD-style 10-year risk proxy. Illustrative and not for
//! clinical use; transparent so it can be audited.

use crate::{PathosError, Result};
use tpt_soma_simulacrum::solver::{OdeSystem, integrate};

/// Two-element Windkessel model of the arterial tree: `C dP/dt = Q(t) - P / R`.
///
/// Inflow `Q(t)` is a sinusoidal pulsatile flow around `q_mean` with amplitude
/// `q_amplitude` (`0 <= q_amplitude <= q_mean`, so flow stays non-negative).
/// Over whole cardiac cycles the mean pressure converges to `R * q_mean`.
#[derive(Debug, Clone, Copy)]
pub struct Windkessel {
    /// Peripheral resistance `R`.
    pub resistance: f64,
    /// Arterial compliance `C`.
    pub compliance: f64,
    /// Mean inflow.
    pub q_mean: f64,
    /// Pulsatile flow amplitude.
    pub q_amplitude: f64,
    /// Cardiac cycle period.
    pub period: f64,
}

impl OdeSystem for Windkessel {
    fn dim(&self) -> usize {
        1
    }

    fn deriv(&self, t: f64, y: &[f64], dydt: &mut [f64]) {
        let phase = 2.0 * std::f64::consts::PI * t / self.period;
        let q = self.q_mean + self.q_amplitude * phase.sin();
        dydt[0] = (q - y[0] / self.resistance) / self.compliance;
    }
}

/// Simulate arterial pressure `(t, P)` under a Windkessel model from `P(0) = 0`.
pub fn windkessel_pressure_trace(
    model: &Windkessel,
    t_end: f64,
    dt: f64,
) -> Result<Vec<(f64, f64)>> {
    if model.resistance <= 0.0 || model.compliance <= 0.0 || model.period <= 0.0 {
        return Err(PathosError::InvalidInput(
            "resistance, compliance, and period must be positive".to_string(),
        ));
    }
    if model.q_mean < 0.0 || !(0.0..=model.q_mean).contains(&model.q_amplitude) {
        return Err(PathosError::InvalidInput(
            "require 0 <= q_amplitude <= q_mean".to_string(),
        ));
    }
    if t_end <= 0.0 || dt <= 0.0 {
        return Err(PathosError::InvalidInput(
            "t_end and dt must be positive".to_string(),
        ));
    }
    let steps = (t_end / dt).round() as usize;
    Ok(integrate(model, 0.0, &[0.0], dt, steps)
        .into_iter()
        .map(|(t, y)| (t, y[0]))
        .collect())
}

/// Simplified ASCVD-style 10-year risk proxy in [0, 1] (illustrative).
///
/// Additive points model from standard risk factors; mapped through a logistic
/// function to a probability. Higher HDL is protective (negative points).
pub fn ascvd_10yr_risk_proxy(
    age: f64,
    is_female: bool,
    total_chol: f64,
    hdl: f64,
    sbp: f64,
    smoker: bool,
    diabetic: bool,
) -> Result<f64> {
    if age <= 0.0 || total_chol <= 0.0 || hdl <= 0.0 || sbp <= 0.0 {
        return Err(PathosError::InvalidInput(
            "age, total_chol, hdl, and sbp must be positive".to_string(),
        ));
    }
    let mut points = (age - 40.0).max(0.0) * 0.02;
    if is_female {
        points -= 2.0;
    }
    points += (total_chol - 180.0).max(0.0) / 40.0;
    points -= (hdl - 50.0).max(0.0) / 10.0;
    points += (sbp - 120.0).max(0.0) / 20.0;
    if smoker {
        points += 2.0;
    }
    if diabetic {
        points += 2.0;
    }
    let risk = 1.0 / (1.0 + (-(points) / 10.0).exp());
    Ok(risk.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windkessel_mean_pressure_equals_resistance_times_mean_flow() {
        // Over whole cycles, mean dP/dt is zero, so mean P = R * Q_mean.
        let model = Windkessel {
            resistance: 1.0,
            compliance: 1.0,
            q_mean: 1.0,
            q_amplitude: 0.5,
            period: 1.0,
        };
        let trace = windkessel_pressure_trace(&model, 30.0, 0.001).unwrap();
        // Discard the transient (a few RC time constants) and keep whole cycles.
        let steady: Vec<f64> = trace
            .iter()
            .filter(|(t, _)| *t >= 20.0)
            .map(|(_, p)| *p)
            .collect();
        let mean = steady.iter().sum::<f64>() / steady.len() as f64;
        assert!((mean - 1.0).abs() < 0.01, "mean pressure {mean}");
    }

    #[test]
    fn test_windkessel_rejects_invalid_model() {
        let bad_amplitude = Windkessel {
            resistance: 1.0,
            compliance: 1.0,
            q_mean: 1.0,
            q_amplitude: 2.0,
            period: 1.0,
        };
        assert!(windkessel_pressure_trace(&bad_amplitude, 5.0, 0.01).is_err());
    }

    #[test]
    fn test_risk_in_range_and_monotone_in_age() {
        let low = ascvd_10yr_risk_proxy(45.0, false, 190.0, 50.0, 125.0, false, false).unwrap();
        let high = ascvd_10yr_risk_proxy(70.0, false, 240.0, 40.0, 150.0, true, true).unwrap();
        assert!((0.0..=1.0).contains(&low));
        assert!((0.0..=1.0).contains(&high));
        assert!(high > low);
    }

    #[test]
    fn test_risk_rejects_nonpositive() {
        assert!(ascvd_10yr_risk_proxy(0.0, false, 190.0, 50.0, 125.0, false, false).is_err());
    }
}
