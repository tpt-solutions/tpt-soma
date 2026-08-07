//! Digital-twin calibration MVP (Phase 3).
//!
//! Fits the parameters of a `ParametricOde` system to empirical trajectory data
//! using gradient descent with numerically-estimated gradients (finite
//! differences). This is the baseline "fit a model to a sample's multi-omics /
//! clinical baseline" path that later phases generalize into the cross-talk
//! solver over the full OSG.

use crate::solver::{State, rk4_step};

/// An ODE system whose dynamics depend on a parameter vector `params`.
pub trait ParametricOde {
    fn dim(&self) -> usize;
    fn deriv(&self, params: &[f64], t: f64, y: &[f64], dydt: &mut [f64]);
}

struct Wrapped<'a, P: ParametricOde + ?Sized> {
    sys: &'a P,
    params: &'a [f64],
}

impl<'a, P: ParametricOde + ?Sized> crate::solver::OdeSystem for Wrapped<'a, P> {
    fn dim(&self) -> usize {
        self.sys.dim()
    }
    fn deriv(&self, t: f64, y: &[f64], dydt: &mut [f64]) {
        self.sys.deriv(self.params, t, y, dydt);
    }
}

/// Integrate a parametric system and return the state at each step.
pub fn simulate_parametric(
    sys: &dyn ParametricOde,
    params: &[f64],
    t0: f64,
    y0: &[f64],
    dt: f64,
    steps: usize,
) -> Vec<State> {
    let w = Wrapped { sys, params };
    let mut out = Vec::with_capacity(steps + 1);
    let mut t = t0;
    let mut y = y0.to_vec();
    out.push(y.clone());
    for _ in 0..steps {
        y = rk4_step(&w, t, &y, dt);
        t += dt;
        out.push(y.clone());
    }
    out
}

/// Sum-of-squared-errors between simulated and observed trajectories.
pub fn sse(
    sys: &dyn ParametricOde,
    params: &[f64],
    t0: f64,
    y0: &[f64],
    dt: f64,
    observed: &[Vec<f64>],
) -> f64 {
    let sim = simulate_parametric(sys, params, t0, y0, dt, observed.len().saturating_sub(1));
    sim.iter()
        .zip(observed.iter())
        .map(|(a, b)| {
            a.iter()
                .zip(b.iter())
                .map(|(x, y)| (x - y).powi(2))
                .sum::<f64>()
        })
        .sum()
}

/// Gradient-descent parameter fit. `observed` is the trajectory at each step
/// (length `steps + 1`). Returns the fitted parameter vector.
///
/// `eps` is the finite-difference step for numerical gradients; `lr` is the
/// learning rate; `iters` is the number of gradient steps.
#[allow(clippy::too_many_arguments)]
pub fn calibrate(
    sys: &dyn ParametricOde,
    y0: &[f64],
    t0: f64,
    dt: f64,
    observed: &[Vec<f64>],
    init: &[f64],
    lr: f64,
    iters: usize,
    eps: f64,
) -> Vec<f64> {
    let mut params = init.to_vec();
    for _ in 0..iters {
        let base = sse(sys, &params, t0, y0, dt, observed);
        let grad: Vec<f64> = (0..params.len())
            .map(|i| {
                let mut pp = params.clone();
                pp[i] += eps;
                let sp = sse(sys, &pp, t0, y0, dt, observed);
                (sp - base) / eps
            })
            .collect();
        for (p, g) in params.iter_mut().zip(grad.iter()) {
            *p -= lr * g;
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exponential decay dy/dt = -k·y, parameterized by `params = [k]`.
    struct Decay;
    impl ParametricOde for Decay {
        fn dim(&self) -> usize {
            1
        }
        fn deriv(&self, params: &[f64], _t: f64, y: &[f64], dydt: &mut [f64]) {
            dydt[0] = -params[0] * y[0];
        }
    }

    #[test]
    fn test_calibrate_recovers_decay_rate() {
        let y0 = vec![1.0];
        let dt = 0.1;
        let steps = 30;
        // Build synthetic "observed" data from the true k = 0.5.
        let true_params = vec![0.5];
        let observed = simulate_parametric(&Decay, &true_params, 0.0, &y0, dt, steps);

        let fitted = calibrate(
            &Decay,
            &y0,
            0.0,
            dt,
            &observed,
            &[1.5], // deliberately wrong start
            0.05,
            400,
            1e-4,
        );
        assert!((fitted[0] - 0.5).abs() < 0.05, "fitted k = {}", fitted[0]);
    }
}
