//! PK/PD and physiological signaling models (Phase 3).
//!
//! `OralOneCompartment` is a standard first-order absorption / first-order
//! elimination PK model. `InsulinGlucose` is a minimal insulin-signaling model
//! used as the digital-twin baseline for metabolic / endocrine digital twins.

use crate::solver::{OdeSystem, State, integrate};
use crate::{Result, SimulacrumError};

/// Oral one-compartment PK model with first-order absorption.
///
/// State: `[A_gut, A_central]`. dA_gut/dt = -ka·A_gut;
/// dA_central/dt = ka·A_gut - ke·A_central. Concentration C = A_central / V.
pub struct OralOneCompartment {
    pub ka: f64, // absorption rate constant (1/time)
    pub ke: f64, // elimination rate constant (1/time)
    pub v: f64,  // volume of distribution
}

impl OdeSystem for OralOneCompartment {
    fn dim(&self) -> usize {
        2
    }
    fn deriv(&self, _t: f64, y: &[f64], dydt: &mut [f64]) {
        dydt[0] = -self.ka * y[0];
        dydt[1] = self.ka * y[0] - self.ke * y[1];
    }
}

impl OralOneCompartment {
    pub fn new(ka: f64, ke: f64, v: f64) -> Result<Self> {
        if ka <= 0.0 || ke <= 0.0 || v <= 0.0 {
            return Err(SimulacrumError::InvalidInput(
                "ka, ke, and v must be positive".to_string(),
            ));
        }
        Ok(Self { ka, ke, v })
    }

    /// Simulate plasma concentration (mass/volume) over `steps` of `dt`.
    pub fn simulate(&self, dose: f64, t0: f64, dt: f64, steps: usize) -> Vec<(f64, f64)> {
        let y0 = vec![dose, 0.0];
        integrate(self, t0, &y0, dt, steps)
            .into_iter()
            .map(|(t, y)| (t, y[1] / self.v))
            .collect()
    }
}

/// Minimal insulin → glucose-uptake signaling model (digital-twin baseline).
///
/// State: `[I, G]` where I = insulin concentration, G = glucose concentration.
/// Insulin drives glucose uptake; glucose has a constant endogenous production.
pub struct InsulinGlucose {
    pub k_uptake: f64, // insulin-driven glucose uptake rate
    pub k_prod: f64,   // endogenous glucose production
    pub k_clear: f64,  // insulin clearance
}

impl OdeSystem for InsulinGlucose {
    fn dim(&self) -> usize {
        2
    }
    fn deriv(&self, _t: f64, y: &[f64], dydt: &mut [f64]) {
        let (i, g) = (y[0], y[1]);
        dydt[0] = -self.k_clear * i;
        dydt[1] = self.k_prod - self.k_uptake * i * g;
    }
}

impl InsulinGlucose {
    pub fn simulate(&self, i0: f64, g0: f64, t0: f64, dt: f64, steps: usize) -> Vec<(f64, State)> {
        integrate(self, t0, &[i0, g0], dt, steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pk_oral_one_compartment_concentration_peak() {
        // With first-order absorption, concentration should rise then fall.
        let pk = OralOneCompartment::new(1.0, 0.2, 10.0).unwrap();
        let traj = pk.simulate(100.0, 0.0, 0.1, 200);
        let mut peak = 0.0_f64;
        for (_, c) in &traj {
            peak = peak.max(*c);
        }
        assert!(peak > 0.0);
        // late-time concentration should be below the peak
        let last = traj.last().unwrap().1;
        assert!(last < peak);
    }

    #[test]
    fn test_insulin_glucose_reaches_low_glucose_under_high_insulin() {
        let model = InsulinGlucose {
            k_uptake: 0.01,
            k_prod: 1.0,
            k_clear: 0.1,
        };
        let traj = model.simulate(
            /* insulin */ 5.0, /* glucose */ 100.0, 0.0, 0.05, 400,
        );
        // Glucose should be driven down from its initial 100 by insulin uptake.
        let final_g = traj.last().unwrap().1[1];
        assert!(final_g < 100.0);
    }
}
