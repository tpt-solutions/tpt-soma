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
fn invalid_sensitivity_is_rejected_and_never_changes_budget() {
    let mut dp = DifferentialPrivacy::new(1.0);
    dp.spend_budget("cohort-a", 0.5).unwrap();
    for bad in [0.0, -0.5, -1e9, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            matches!(
                dp.spend_budget("cohort-a", bad),
                Err(BudgetError::InvalidSensitivity)
            ),
            "{bad} should be rejected"
        );
        assert!(!dp.can_spend("cohort-a", bad), "{bad} should not fit");
    }
    // A negative spend would have refunded epsilon. The budget must be unchanged.
    assert!((dp.remaining_budget("cohort-a") - 0.5).abs() < 1e-9);
}

#[test]
fn can_spend_matches_spend_without_spending() {
    let mut dp = DifferentialPrivacy::new(1.0);
    dp.spend_budget("cohort-a", 0.6).unwrap();
    assert!(dp.can_spend("cohort-a", 0.4));
    assert!(!dp.can_spend("cohort-a", 0.5));
    // The dry run does not move the budget.
    assert!((dp.remaining_budget("cohort-a") - 0.4).abs() < 1e-9);
    assert!(dp.spend_budget("cohort-a", 0.4).is_ok());
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
