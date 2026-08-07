//! tpt-soma-simulacrum: Computational & digital-twin core (Phase 3).
//!
//! Provides an ODE/PDE solver framework, PK/PD models (absorption /
//! distribution / metabolism / excretion), and a digital-twin calibration MVP
//! that fits model parameters to empirical multi-omics / clinical baselines via
//! gradient descent. The general solver is floating point per the roadmap's
//! descoping decision; exact/rational arithmetic is scoped narrowly to
//! audit-sensitive flux calculations (left as a follow-up).

pub mod calibration;
pub mod crosstalk;
pub mod error;
pub mod pkpd;
pub mod solver;
pub mod storage;

pub use error::{Result, SimulacrumError};

/// Data class introduced in Phase 3 for simulation-derived outputs.
pub const DATA_CLASS_SIMULATION_OUTPUT: &str = "simulation_output";

#[cfg(test)]
mod tests {
    #[test]
    fn test_data_class_constant() {
        assert_eq!(crate::DATA_CLASS_SIMULATION_OUTPUT, "simulation_output");
    }
}
