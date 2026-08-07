# Differential Privacy (DP)

## Summary

`tpt-soma` provides cohort-level aggregate exports to researchers while
limiting the information that can be learned about any single participant.
Aggregate exports go through a **single enforcement code path** that injects
Laplace noise and draws against a per-cohort epsilon budget.

## Mechanism

### Laplace mechanism

For an aggregate such as a cohort count, we compute the true value and add
noise drawn from a Laplace distribution:

```
scale = sensitivity / epsilon
noisy = true_value + Laplace(scale)
```

Implemented in `tpt_soma_core::dp::DifferentialPrivacy::laplace_noise` /
`noisy_count`.

### Sensitivity

Sensitivity is the maximum amount the true aggregate can change when one
participant is added or removed. For counts, sensitivity is 1 by default; the
API accepts an explicit `sensitivity` on the aggregate request so callers must
state their worst-case individual contribution.

### Epsilon budget

- Each cohort has a global epsilon budget (`DP_EPSILON`, default 1.0).
- Every aggregate export spends an amount equal to its sensitivity from the
  cohort's budget.
- When a cohort's cumulative spend would exceed its budget, further exports are
  rejected (`BudgetError::Exhausted` → HTTP 403). This bounds the cumulative
  privacy loss per cohort, per deployment lifetime.

### Enforcement path

All cohort aggregate exports must call
`DifferentialPrivacyService::cohort_aggregate_export`, which:

1. spends budget (`spend_budget`),
2. records the spend through the audit ledger via the
   `record_dp_budget_spend` hook,
3. returns the noisy aggregate.

The only public endpoint that currently exercises this path is
`POST /api/v1/cohorts/:cohort_id/aggregate/count`. Future aggregate exports
(simulation-derived, cross-domain) must route through the same
`cohort_aggregate_export` code path (see cross-phase dependency #7 in the
project TODO).

## What DP protects — and does not

- **Protects:** re-identification of a single participant from cohort-level
  aggregate outputs, bounded per-cohort privacy loss over time.
- **Does not protect:** per-sample raw data exports (which are gated by CBAC,
  not DP), nor composition attacks across many correlated queries beyond the
  cumulative budget cap. Future work should add richer DP composition/adaptive
  stopping if the pilot uses more than simple counts.

## Relevant code

- `crates/tpt-soma-core/src/dp.rs` — mechanism, budget, service with audit hook
- `crates/tpt-soma-api/src/server.rs` (`cohort_aggregate_count`) — the single
  enforcement code path
- `crates/tpt-soma-api/src/auth.rs` (`record_dp_budget_spend`) — audit hook
- `crates/tpt-soma-core/tests/dp_tests.rs` — noise distribution + budget
  exhaustion tests
