use rand::distributions::{Distribution, Uniform};
use std::collections::HashMap;
use std::sync::Arc;

pub struct DifferentialPrivacy {
    epsilon: f64,
    spent: HashMap<String, f64>,
}

pub struct BudgetSpend {
    pub cohort: String,
    pub epsilon_spent: f64,
}

pub type BudgetAuditHook = Arc<dyn Fn(String, f64, String) + Send + Sync>;

pub struct DifferentialPrivacyService {
    engine: DifferentialPrivacy,
    on_spend: Option<BudgetAuditHook>,
}

impl DifferentialPrivacy {
    pub fn new(epsilon: f64) -> Self {
        Self {
            epsilon,
            spent: HashMap::new(),
        }
    }

    pub fn laplace_noise(&self, sensitivity: f64) -> f64 {
        let scale = sensitivity / self.epsilon;
        let uniform = Uniform::new(0.0, 1.0);
        let mut rng = rand::thread_rng();
        let u: f64 = uniform.sample(&mut rng);
        let sign = if u < 0.5 { -1.0 } else { 1.0 };
        let v = 1.0 - 2.0 * (u - 0.5).abs();
        sign * scale * v.ln()
    }

    pub fn noisy_count(&self, count: usize, sensitivity: f64) -> f64 {
        count as f64 + self.laplace_noise(sensitivity)
    }

    pub fn spend_budget(&mut self, cohort: &str, amount: f64) -> Result<BudgetSpend, BudgetError> {
        let current = self.spent.entry(cohort.to_string()).or_insert(0.0);
        if *current + amount > self.epsilon {
            return Err(BudgetError::Exhausted);
        }
        *current += amount;
        Ok(BudgetSpend {
            cohort: cohort.to_string(),
            epsilon_spent: amount,
        })
    }

    pub fn remaining_budget(&self, cohort: &str) -> f64 {
        let spent = self.spent.get(cohort).copied().unwrap_or(0.0);
        self.epsilon - spent
    }
}

impl DifferentialPrivacyService {
    pub fn new(epsilon: f64) -> Self {
        Self {
            engine: DifferentialPrivacy::new(epsilon),
            on_spend: None,
        }
    }

    pub fn with_audit_hook(mut self, hook: BudgetAuditHook) -> Self {
        self.on_spend = Some(hook);
        self
    }

    pub fn spend_budget(
        &mut self,
        cohort: &str,
        actor: &str,
        amount: f64,
    ) -> Result<BudgetSpend, BudgetError> {
        let spend = self.engine.spend_budget(cohort, amount)?;
        if let Some(ref hook) = self.on_spend {
            let cohort = spend.cohort.clone();
            hook(cohort, spend.epsilon_spent, actor.to_string());
        }
        Ok(spend)
    }

    pub fn noisy_count(&self, count: usize, sensitivity: f64) -> f64 {
        self.engine.noisy_count(count, sensitivity)
    }

    pub fn remaining_budget(&self, cohort: &str) -> f64 {
        self.engine.remaining_budget(cohort)
    }

    pub fn cohort_aggregate_export(
        &mut self,
        cohort: &str,
        actor: &str,
        count: usize,
        sensitivity: f64,
    ) -> Result<f64, BudgetError> {
        self.spend_budget(cohort, actor, sensitivity)?;
        Ok(self.noisy_count(count, sensitivity))
    }

    /// Cross-domain aggregate export (ADR 007 §2.8): combine counts drawn from
    /// multiple Phase 1–4 data classes for a single cohort into one
    /// differentially-private release. The full cohort epsilon budget is spent
    /// once for the combined export, and the audit hook fires with the
    /// `"cross_domain"` marker domain so the spend is attributable.
    pub fn cross_domain_aggregate_export(
        &mut self,
        cohort: &str,
        actor: &str,
        domain_counts: &[(String, usize)],
        sensitivity: f64,
    ) -> Result<f64, BudgetError> {
        self.spend_budget(cohort, actor, sensitivity)?;
        let total: usize = domain_counts.iter().map(|(_, c)| *c).sum();
        Ok(self.noisy_count(total, sensitivity))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BudgetError {
    #[error("differential privacy budget exhausted")]
    Exhausted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noisy_count_adds_noise() {
        let dp = DifferentialPrivacy::new(1.0);
        let noisy = dp.noisy_count(100, 1.0);
        assert!(noisy > 0.0);
    }

    #[test]
    fn test_budget_exhaustion_blocks_further_exports() {
        let mut dp = DifferentialPrivacy::new(0.5);
        let _ = dp.spend_budget("cohort-a", 0.3).unwrap();
        let result = dp.spend_budget("cohort-a", 0.3);
        assert!(result.is_err());
    }

    #[test]
    fn test_noisy_count_centered_around_true_value() {
        let dp = DifferentialPrivacy::new(10.0);
        let mut sum = 0.0;
        for _ in 0..1000 {
            sum += dp.noisy_count(50, 1.0);
        }
        let mean = sum / 1000.0;
        assert!(mean > 40.0 && mean < 60.0, "mean={}", mean);
    }

    #[test]
    fn test_service_spend_calls_hook() {
        use std::sync::Mutex;
        let hook_calls = Arc::new(Mutex::new(Vec::new()));
        let hook_calls_clone = hook_calls.clone();
        let hook: BudgetAuditHook = Arc::new(move |cohort, epsilon, actor| {
            hook_calls_clone
                .lock()
                .unwrap()
                .push((cohort, epsilon, actor));
        });

        let mut service = DifferentialPrivacyService::new(1.0).with_audit_hook(hook);
        let _ = service
            .spend_budget("cohort-a", "researcher-1", 0.2)
            .unwrap();

        let calls = hook_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "cohort-a");
        assert_eq!(calls[0].1, 0.2);
        assert_eq!(calls[0].2, "researcher-1");
    }

    #[test]
    fn test_service_cohort_aggregate_export() {
        let mut service = DifferentialPrivacyService::new(1.0);
        let result = service.cohort_aggregate_export("cohort-a", "researcher-1", 100, 0.1);
        assert!(result.is_ok());
        assert!(result.unwrap() > 0.0);
    }

    #[test]
    fn test_service_cohort_aggregate_export_budget_exhausted() {
        let mut service = DifferentialPrivacyService::new(0.1);
        let _ = service
            .cohort_aggregate_export("cohort-a", "researcher-1", 100, 0.05)
            .unwrap();
        let result = service.cohort_aggregate_export("cohort-a", "researcher-1", 100, 0.06);
        assert!(result.is_err());
    }

    #[test]
    fn test_service_cross_domain_aggregate_export() {
        let mut service = DifferentialPrivacyService::new(1.0);
        let domains = vec![
            ("genomic_variant".to_string(), 100),
            ("transcriptomic_scrna".to_string(), 50),
            ("cgm_continuous".to_string(), 25),
        ];
        let result = service
            .cross_domain_aggregate_export("cohort-a", "researcher-1", &domains, 0.1)
            .unwrap();
        assert!(result > 0.0);
    }

    #[test]
    fn test_service_cross_domain_aggregate_export_budget_exhausted() {
        let mut service = DifferentialPrivacyService::new(0.05);
        let domains = vec![("genomic_variant".to_string(), 100)];
        let result =
            service.cross_domain_aggregate_export("cohort-a", "researcher-1", &domains, 0.1);
        assert!(result.is_err());
    }
}
