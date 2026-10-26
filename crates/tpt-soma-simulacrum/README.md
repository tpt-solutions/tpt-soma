# tpt-soma-simulacrum

ODE/PDE solver, PK/PD models, digital-twin calibration, and OSG cross-talk solver.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-simulacrum` is the computational core. It integrates ordinary differential equations (ODEs) and a one-dimensional diffusion PDE, runs pharmacokinetic and pharmacodynamic (PK/PD) models, fits model parameters to observed baselines, and solves cross-talk between organ systems in the organ-system graph (OSG). Simulation results are stored as data classes and can be mirrored into Chronos for time-series queries.

## Features

- **ODE solvers** (`solver`): fixed-step `euler_step` and `rk4_step`, and `integrate` for a full trajectory. The `OdeSystem` trait defines the right-hand side. `ParametricOde` adds a parameter vector, which is what calibration uses.
- **PDE solver**: `solve_1d_diffusion` steps a one-dimensional diffusion equation on a grid.
- **PK/PD models** (`pkpd`): `OralOneCompartment` for oral absorption and elimination, and `InsulinGlucose` for the insulin-glucose feedback system. `simulate` and `simulate_parametric` run them.
- **Digital-twin calibration** (`calibration`): `calibrate` fits parameters by gradient descent, with finite-difference gradients, against `CalibrationTarget` values. `sse` gives the sum-of-squares error being minimized, and `ParameterSet` records the result.
- **OSG cross-talk** (`crosstalk`): `OsTopology` describes organ-system nodes (`OsNode`, `OsKind`) and edges (`OsEdge`, with an `EdgeCoupling`). `CrossTalkSystem` integrates the coupled system, `build_from_topology` constructs it, and `build_adipose_igf1_breast` builds the reference adipose-IGF1-breast topology. `run_systemic_query` answers a `SystemicQuery`.
- **Storage** (`storage`): `insert_simulation_run`, `insert_simulation_output`, `insert_parameter_set`, `insert_calibration_target`, and `get_simulation_outputs` persist and read results. `mirror_outputs_to_chronos` and `mirror_run_to_plexus` copy results into the time-series and graph tables.

## Numerical approach

The general solver uses floating point. This is the descoping decision recorded in the crate docs: exact or rational arithmetic is reserved for audit-sensitive flux calculations and is not yet implemented. Step sizes and tolerances are set by the caller, so check convergence (for example by halving the step) before trusting a result.

## Data classes

| Constant | Value | Use |
|---|---|---|
| `DATA_CLASS_SIMULATION_OUTPUT` | `simulation_output` | Simulation outputs and systemic query results |

Simulation outputs are released through the same differential-privacy path as other aggregates when they are shared across a cohort, as described in [`tpt-soma-core`](../tpt-soma-core/README.md).

## Crate layout

| Module | Responsibility |
|---|---|
| `solver` | `OdeSystem`, `ParametricOde`, Euler and RK4 steps, `integrate`, the 1D diffusion solver |
| `pkpd` | PK/PD models and their simulation functions |
| `calibration` | `CalibrationTarget`, `ParameterSet`, `calibrate`, `sse` |
| `crosstalk` | OSG topology, `CrossTalkSystem`, reference topology builder, systemic queries |
| `storage` | Postgres persistence for runs, outputs and parameters |
| `error` | `SimulacrumError`, `Result` |

## Running the tests

```sh
cargo test -p tpt-soma-simulacrum
```

`tests/systemic_query.rs` checks the cross-talk solver end to end.

## Related crates

- [`tpt-soma-chronos`](../tpt-soma-chronos/README.md): receives mirrored simulation time series
- [`tpt-soma-pathos`](../tpt-soma-pathos/README.md): disease models that can be fitted here
- [`tpt-soma-organon`](../tpt-soma-organon/README.md): the organ-system graph and cascade model
- [`tpt-soma-api`](../tpt-soma-api/README.md): exposes `simulate` and systemic query routes

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
