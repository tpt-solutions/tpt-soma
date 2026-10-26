# tpt-soma-clinica

Clinical trial design and management, cohort discovery, and biomarker discovery/validation statistics.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-clinica` supports the clinical side of research on the platform. It screens subjects against trial eligibility criteria, enrolls them up to a target, records adverse events and summarizes them, finds cohorts from clinical observations, and tests candidate biomarkers against outcomes. The statistics are implemented here rather than pulled in from a large numerical library, so the formulas are short enough to read and check.

## Features

### Statistics (`stats`)

- `mean` and `sample_variance`
- `welch_t_test` for two groups with unequal variance
- `pearson_r` for linear correlation
- `point_biserial_correlation` for a continuous measure against a binary group

### Trials (`trial`)

- **Eligibility:** `Criterion` describes an inclusion or exclusion rule over a subject feature (`SubjectFeatures`). An inclusion criterion fails when the value is missing, and an exclusion criterion does not trigger on a missing value. `evaluate_eligibility` returns an `EligibilityResult`, which lists the failed criteria.
- **Screening and recruitment:** `screen_and_recruit` enrolls the first eligible subjects up to a target and returns a `RecruitmentReport`. `screen_and_persist` does the same and writes the decisions to Postgres.
- **Adverse events:** `AdverseEvent` records a term, a CTCAE-style grade from 1 to 5, and whether it was serious. `summarize_adverse_events` returns an `AeSummary` with counts by grade, grade 3 or higher, serious events and subjects affected. Grades outside 1 to 5 are rejected.

### Cohorts (`cohort`)

- `build_cohort_query` turns a `BiomarkerCohortFilter` into a query over clinical observations.
- `discover_cohort` runs it and returns the matching subjects.
- `insert_clinical_trial_cohort` stores a cohort against a trial.

### Biomarkers (`biomarker`)

- `associate_biomarker` compares a candidate marker between two groups with Welch's t-test and returns a `BiomarkerResult` with the test statistic, p-value and effect size.
- `record_biomarker` stores the result in the `biomarker_discovery` class.

### Storage (`storage`)

Persistence for trial protocols, trial criteria, screening decisions, adverse events and biomarker results. Tables are created by migration 10 in [`tpt-soma-core`](../tpt-soma-core/README.md), which covers the clinical trial results schema. Adverse-event summaries are read back with `load_ae_summary`.

## Data classes

Trial outputs use the `clinical_trial` class and biomarker outputs use `biomarker_discovery`. Their constants (`DATA_CLASS_CLINICAL_TRIAL`, `DATA_CLASS_BIOMARKER`) are defined in [`tpt-soma-pathos`](../tpt-soma-pathos/README.md). The trial routes in [`tpt-soma-api`](../tpt-soma-api/README.md) apply the same capability and audit checks as the rest of the platform.

## Errors

`ClinicaError` covers invalid input (for example a grade out of range), statistics errors (`Stats`), database errors and core errors. `Result` is the crate's alias for it.

## Crate layout

| Module | Responsibility |
|---|---|
| `stats` | Descriptive and inferential statistics |
| `trial` | Eligibility, screening, recruitment and adverse events |
| `cohort` | Cohort query building and discovery |
| `biomarker` | Biomarker association and recording |
| `storage` | Postgres persistence |
| `error` | `ClinicaError`, `Result` |

## Running the tests

```sh
cargo test -p tpt-soma-clinica
```

Most trial and statistics logic is pure and tested without a database. Persistence needs a running database.

## Status

This crate is under active development. Trial persistence, the adverse-event endpoints and the screening API are recent. Check [`TODO.md`](../../TODO.md) for the open items before relying on a path in production.

## Related crates

- [`tpt-soma-organon`](../tpt-soma-organon/README.md): the clinical observations that cohorts are built from
- [`tpt-soma-harmonize`](../tpt-soma-harmonize/README.md): codes used in eligibility criteria
- [`tpt-soma-pathos`](../tpt-soma-pathos/README.md): models that biomarker candidates can come from
- [`tpt-soma-audit`](../tpt-soma-audit/README.md): records each trial and cohort access

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
