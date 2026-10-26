# tpt-soma-pathos

Computational pathology: metabolic/endocrine, oncology, longevity, and cardiovascular models.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-pathos` turns measured data into disease-level estimates. Each sub-module covers one area of disease modelling and exposes plain functions that take clinical inputs and return scores, classifications or fitted curves. The models are small, published formulas and simple ODE-style dynamics; they are decision aids, not diagnoses.

## Modules

| Module | Models |
|---|---|
| `metabolic` | Insulin resistance (`homa_ir`), metabolic syndrome classification (`metabolic_syndrome_assessment`, NCEP ATP III), and a combined insulin-resistance risk score (`insulin_resistance_risk_score`) |
| `oncology` | Gompertz tumor growth (`gompertz_closed_form`, `simulate_gompertz_growth`), RECIST response (`recist_response`), tumor microenvironment immunosuppression (`tme_immunosuppression_score`), and an immunotherapy response proxy (`immunotherapy_response_proxy`) |
| `autoimmune` | Disease activity for rheumatoid arthritis (`das28_crp`, `classify_das28`) |
| `cardiovascular` | Windkessel arterial pressure (`windkessel_pressure_trace`) and a 10-year ASCVD risk proxy (`ascvd_10yr_risk_proxy`) |
| `infectious` | SIR epidemic dynamics (`simulate_sir`, `sir_final_size`, `basic_reproduction_number`) |
| `longevity` | Phenotypic age proxy (`phenotypic_age_proxy`), senescence index (`senescence_index`), Gompertz mortality hazard (`hazard`) and its fit (`fit_gompertz_hazard`), and doubling time (`doubling_time`) |

The `Gompertz`, `GompertzHazard`, `Sir`, `Windkessel`, `Das28Input`, `MetabolicSyndromeInput` and `MetabolicSyndromeResult` types hold the inputs and fitted parameters.

## Data classes

| Constant | Value | Use |
|---|---|---|
| `DATA_CLASS_PATHOS_FINDING` | `pathos_finding` | Model outputs |
| `DATA_CLASS_CLINICAL_TRIAL` | `clinical_trial` | Trial design and cohort metadata |
| `DATA_CLASS_BIOMARKER` | `biomarker_discovery` | Biomarker association outputs |

Results are written under these classes, so access to them goes through the same [capability](../tpt-soma-capability/README.md) and [audit](../tpt-soma-audit/README.md) checks as the rest of the platform.

## Scope

These are first-slice implementations of each model's core formulas and dynamics. They are not calibrated against any particular population. Treat outputs as research estimates until a model is validated against local data. Parameters can be fitted with [`tpt-soma-simulacrum`](../tpt-soma-simulacrum/README.md).

## Crate layout

Each sub-module is a single file under `src/`, and `error.rs` defines `PathosError`. `lib.rs` re-exports `PathosError` and `Result`.

## Running the tests

```sh
cargo test -p tpt-soma-pathos
```

## Related crates

- [`tpt-soma-organon`](../tpt-soma-organon/README.md): the observations these models consume
- [`tpt-soma-simulacrum`](../tpt-soma-simulacrum/README.md): the ODE solver and calibration used for fitting
- [`tpt-soma-chronos`](../tpt-soma-chronos/README.md): CGM data used for glycemic models

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
