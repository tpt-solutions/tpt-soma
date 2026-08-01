use tpt_soma_core::dp::{DifferentialPrivacy, BudgetError};

#[test]
fn noisy_count_adds_noise() {
    let dp = DifferentialPrivacy::new(0.1);
    let noisy = dp.noisy_count(100, 1.0);
    assert!(noisy > 0.0, "noisy count should be positive");
}

#[test]
fn budget_exhaustion_blocks_further_exports() {
    let mut dp = DifferentialPrivacy::new(1.0);
    dp.spend_budget("cohort-a", 0.5).unwrap();
    dp.spend_budget("cohort-a", 0.4).unwrap();
    let result = dp.spend_budget("cohort-a", 0.2);
    assert!(matches!(result, Err(BudgetError::Exhausted)));
}
