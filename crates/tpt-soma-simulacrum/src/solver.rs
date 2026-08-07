//! ODE / PDE solver framework.
//!
//! A minimal but correct numerical-integration toolkit: explicit Euler and
//! classic RK4 for ODE systems, plus an explicit FTCS solver for 1-D diffusion
//! (PDE). These back the PK/PD and digital-twin models in this crate.

pub type State = Vec<f64>;

/// A first-order ODE system `dy/dt = f(t, y)`.
pub trait OdeSystem {
    fn dim(&self) -> usize;
    fn deriv(&self, t: f64, y: &[f64], dydt: &mut [f64]);
}

/// Single explicit-Euler step.
pub fn euler_step<S: OdeSystem>(sys: &S, t: f64, y: &[f64], dt: f64) -> State {
    let mut dydt = vec![0.0; y.len()];
    sys.deriv(t, y, &mut dydt);
    y.iter()
        .zip(dydt.iter())
        .map(|(yi, d)| yi + d * dt)
        .collect()
}

/// Single classic 4th-order Runge-Kutta step.
pub fn rk4_step<S: OdeSystem>(sys: &S, t: f64, y: &[f64], dt: f64) -> State {
    let n = y.len();
    let mut k1 = vec![0.0; n];
    sys.deriv(t, y, &mut k1);
    let y2: Vec<f64> = y
        .iter()
        .zip(&k1)
        .map(|(yi, ki)| yi + 0.5 * dt * ki)
        .collect();
    let mut k2 = vec![0.0; n];
    sys.deriv(t + 0.5 * dt, &y2, &mut k2);
    let y3: Vec<f64> = y
        .iter()
        .zip(&k2)
        .map(|(yi, ki)| yi + 0.5 * dt * ki)
        .collect();
    let mut k3 = vec![0.0; n];
    sys.deriv(t + 0.5 * dt, &y3, &mut k3);
    let y4: Vec<f64> = y.iter().zip(&k3).map(|(yi, ki)| yi + dt * ki).collect();
    let mut k4 = vec![0.0; n];
    sys.deriv(t + dt, &y4, &mut k4);
    y.iter()
        .zip(k1.iter())
        .zip(k2.iter())
        .zip(k3.iter())
        .zip(k4.iter())
        .map(|((((yi, a), b), c), d)| yi + dt / 6.0 * (a + 2.0 * b + 2.0 * c + d))
        .collect()
}

/// Integrate an ODE system with RK4, returning `(t, y)` samples.
pub fn integrate<S: OdeSystem>(
    sys: &S,
    t0: f64,
    y0: &[f64],
    dt: f64,
    steps: usize,
) -> Vec<(f64, State)> {
    let mut out = Vec::with_capacity(steps + 1);
    let mut t = t0;
    let mut y = y0.to_vec();
    out.push((t, y.clone()));
    for _ in 0..steps {
        y = rk4_step(sys, t, &y, dt);
        t += dt;
        out.push((t, y.clone()));
    }
    out
}

/// Explicit FTCS solver for the 1-D diffusion equation `du/dt = D d^2u/dx^2`.
///
/// Uses zero-flux (Neumann) boundaries. Stability requires
/// `D * dt / dx^2 <= 0.5`; callers should ensure this or the solution diverges.
#[allow(clippy::needless_range_loop)]
pub fn solve_1d_diffusion(
    initial: &[f64],
    dx: f64,
    dt: f64,
    diffusivity: f64,
    steps: usize,
) -> Vec<State> {
    let mut u = initial.to_vec();
    let nx = u.len();
    let alpha = diffusivity * dt / (dx * dx);
    let mut frames = Vec::with_capacity(steps + 1);
    frames.push(u.clone());
    for _ in 0..steps {
        let mut un = u.clone();
        for i in 1..nx.saturating_sub(1) {
            un[i] = u[i] + alpha * (u[i + 1] - 2.0 * u[i] + u[i - 1]);
        }
        if nx >= 2 {
            un[0] = u[0] + alpha * (u[1] - u[0]);
            un[nx - 1] = u[nx - 1] + alpha * (u[nx - 2] - u[nx - 1]);
        }
        u = un;
        frames.push(u.clone());
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ExponentialDecay {
        k: f64,
    }
    impl OdeSystem for ExponentialDecay {
        fn dim(&self) -> usize {
            1
        }
        fn deriv(&self, _t: f64, y: &[f64], dydt: &mut [f64]) {
            dydt[0] = -self.k * y[0];
        }
    }

    #[test]
    fn test_rk4_matches_analytical_exponential_decay() {
        let sys = ExponentialDecay { k: 0.5 };
        let dt = 0.01;
        let steps = 100; // t = 1.0
        let traj = integrate(&sys, 0.0, &[1.0], dt, steps);
        let y = traj.last().unwrap().1[0];
        let expected = (-0.5_f64).exp(); // e^{-0.5}
        assert!((y - expected).abs() < 1e-3, "got {y}, expected {expected}");
    }

    #[test]
    fn test_euler_less_accurate_than_rk4() {
        let sys = ExponentialDecay { k: 1.0 };
        let dt = 0.1;
        let steps = 10; // t = 1.0
        let y_euler = {
            let mut y = vec![1.0];
            for _ in 0..steps {
                y = euler_step(&sys, 0.0, &y, dt);
            }
            y[0]
        };
        let y_rk4 = integrate(&sys, 0.0, &[1.0], dt, steps).last().unwrap().1[0];
        let expected = (-1.0_f64).exp();
        // RK4 should be much closer to e^-1 than Euler at this coarse step.
        assert!((y_rk4 - expected).abs() < (y_euler - expected).abs());
    }

    #[test]
    fn test_1d_diffusion_conserves_and_smooths() {
        let initial = [1.0, 0.0, 0.0, 0.0, 0.0];
        let frames = solve_1d_diffusion(&initial, 1.0, 0.1, 0.2, 5);
        let last = frames.last().unwrap();
        // total "mass" should be roughly conserved under zero-flux BC
        let mass0: f64 = initial.iter().sum();
        let mass1: f64 = last.iter().sum();
        assert!((mass0 - mass1).abs() < 1e-6);
        // the peak should have spread (center value increased from 0)
        assert!(last[1] > 0.0);
    }
}
