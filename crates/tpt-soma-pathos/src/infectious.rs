//! Infectious-disease models (Phase 4, `tpt-soma-pathos`).
//!
//! Deterministic SIR compartment model over population fractions (S + I + R = 1),
//! integrated with the shared `tpt-soma-simulacrum` RK4 solver.

use crate::{PathosError, Result};
use tpt_soma_simulacrum::solver::{OdeSystem, integrate};

/// SIR compartment model: `dS/dt = -β S I`, `dI/dt = β S I - γ I`, `dR/dt = γ I`.
#[derive(Debug, Clone, Copy)]
pub struct Sir {
    /// Transmission rate per day.
    pub beta: f64,
    /// Recovery rate per day.
    pub gamma: f64,
}

impl OdeSystem for Sir {
    fn dim(&self) -> usize {
        3
    }

    fn deriv(&self, _t: f64, y: &[f64], dydt: &mut [f64]) {
        let infection = self.beta * y[0] * y[1];
        dydt[0] = -infection;
        dydt[1] = infection - self.gamma * y[1];
        dydt[2] = self.gamma * y[1];
    }
}

/// Basic reproduction number `R0 = β / γ`.
pub fn basic_reproduction_number(beta: f64, gamma: f64) -> Result<f64> {
    if beta <= 0.0 || gamma <= 0.0 {
        return Err(PathosError::InvalidInput(
            "beta and gamma must be positive".to_string(),
        ));
    }
    Ok(beta / gamma)
}

/// Final epidemic size (fraction ever infected) for a well-mixed SIR model.
///
/// Solves the transcendental relation `z = 1 - exp(-R0 z)` by fixed-point
/// iteration. Returns 0 when `R0 <= 1` (no epidemic).
pub fn sir_final_size(r0: f64) -> Result<f64> {
    if r0 <= 0.0 {
        return Err(PathosError::InvalidInput("r0 must be positive".to_string()));
    }
    if r0 <= 1.0 {
        return Ok(0.0);
    }
    let mut z = 0.5;
    for _ in 0..100_000 {
        let next = 1.0 - (-r0 * z).exp();
        if (next - z).abs() < 1e-14 {
            return Ok(next);
        }
        z = next;
    }
    Ok(z)
}

/// Simulate an SIR epidemic from an initial infected fraction `i0`.
///
/// Returns `(t, [S, I, R])` samples from `t = 0` through `t_end` with step `dt`.
pub fn simulate_sir(
    beta: f64,
    gamma: f64,
    i0: f64,
    t_end: f64,
    dt: f64,
) -> Result<Vec<(f64, Vec<f64>)>> {
    basic_reproduction_number(beta, gamma)?;
    if !(0.0..1.0).contains(&i0) || i0 == 0.0 {
        return Err(PathosError::InvalidInput(
            "i0 must be in (0, 1)".to_string(),
        ));
    }
    if t_end <= 0.0 || dt <= 0.0 {
        return Err(PathosError::InvalidInput(
            "t_end and dt must be positive".to_string(),
        ));
    }
    let steps = (t_end / dt).round() as usize;
    let sys = Sir { beta, gamma };
    Ok(integrate(&sys, 0.0, &[1.0 - i0, i0, 0.0], dt, steps))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_reproduction_number() {
        assert!((basic_reproduction_number(0.5, 0.2).unwrap() - 2.5).abs() < 1e-12);
        assert!(basic_reproduction_number(0.0, 0.2).is_err());
    }

    #[test]
    fn test_final_size_matches_known_value() {
        // Known well-mixed SIR final size for R0 = 2.5 is ~0.8920.
        let z = sir_final_size(2.5).unwrap();
        assert!((z - 0.8920).abs() < 1e-3, "got {z}");
        assert_eq!(sir_final_size(0.9).unwrap(), 0.0);
    }

    #[test]
    fn test_simulated_final_size_matches_analytic() {
        let traj = simulate_sir(0.5, 0.2, 1e-4, 300.0, 0.01).unwrap();
        let (_, last) = traj.last().unwrap();
        let analytic = sir_final_size(2.5).unwrap();
        assert!(
            (last[2] - analytic).abs() < 0.01,
            "simulated {} vs analytic {}",
            last[2],
            analytic
        );
    }

    #[test]
    fn test_population_is_conserved() {
        let traj = simulate_sir(0.5, 0.2, 1e-3, 100.0, 0.05).unwrap();
        for (_, y) in &traj {
            let total: f64 = y.iter().sum();
            assert!((total - 1.0).abs() < 1e-9, "total {total}");
        }
    }

    #[test]
    fn test_subcritical_outbreak_dies_out() {
        // R0 = 0.5: infection decays monotonically and almost nobody is infected.
        let traj = simulate_sir(0.1, 0.2, 1e-2, 200.0, 0.05).unwrap();
        let infected: Vec<f64> = traj.iter().map(|(_, y)| y[1]).collect();
        assert!(infected.windows(2).all(|w| w[1] <= w[0] + 1e-12));
        assert!(traj.last().unwrap().1[2] < 0.02);
    }

    #[test]
    fn test_simulate_sir_rejects_invalid_inputs() {
        assert!(simulate_sir(0.5, 0.2, 0.0, 10.0, 0.1).is_err());
        assert!(simulate_sir(0.5, 0.2, 1.0, 10.0, 0.1).is_err());
        assert!(simulate_sir(0.5, 0.2, 0.01, 0.0, 0.1).is_err());
    }
}
