use tpt_soma_core::dp::{BudgetError, DifferentialPrivacy};

#[test]
fn noisy_count_adds_noise() {
    let dp = DifferentialPrivacy::new(1.0);
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

#[test]
fn noisy_count_centered_around_true_value() {
    let dp = DifferentialPrivacy::new(10.0);
    let mut sum = 0.0;
    for _ in 0..1000 {
        sum += dp.noisy_count(100, 1.0);
    }
    let mean = sum / 1000.0;
    assert!(
        mean > 90.0 && mean < 110.0,
        "mean noisy count should be close to true value, got {}",
        mean
    );
}
