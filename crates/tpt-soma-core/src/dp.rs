use std::collections::HashMap;
use rand::distributions::{Distribution, Uniform};

pub struct DifferentialPrivacy {
    epsilon: f64,
    spent: HashMap<String, f64>,
}

impl DifferentialPrivacy {
    pub fn new(epsilon: f64) -> Self {
        Self { epsilon, spent: HashMap::new() }
    }

    pub fn laplace_noise(&self, sensitivity: f64) -> f64 {
        let scale = sensitivity / self.epsilon;
        let uniform = Uniform::new(0.0, 1.0);
        let mut rng = rand::thread_rng();
        let u: f64 = uniform.sample(&mut rng);
        let sign = if u < 0.5 { -1.0 } else { 1.0 };
        let exp = -self.epsilon * (u - 0.5).abs() / sensitivity;
        sign * scale * exp.ln()
    }

    pub fn noisy_count(&self, count: usize, sensitivity: f64) -> f64 {
        count as f64 + self.laplace_noise(sensitivity)
    }

    pub fn spend_budget(&mut self, cohort: &str, amount: f64) -> Result<(), BudgetError> {
        let current = self.spent.entry(cohort.to_string()).or_insert(0.0);
        if *current + amount > self.epsilon {
            return Err(BudgetError::Exhausted);
        }
        *current += amount;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BudgetError {
    #[error("differential privacy budget exhausted")]
    Exhausted,
}
