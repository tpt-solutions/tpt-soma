# tpt-soma-chronos

Time-series/CGM engine: resampling, glycemic variability, longitudinal trajectories.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-chronos` handles data that arrives as a sequence of timestamped measurements: continuous glucose monitor (CGM) readings and longitudinal organ-function trajectories. It parses vendor exports, cleans and resamples them, computes standard glycemic metrics, and stores results for the rest of the platform.

## Features

- **CGM parsing** (`cgm`, `dexcom`, `libre`): `parse_dexcom_csv` / `parse_dexcom_stream` and `parse_libre_csv` / `parse_libre_stream` read Dexcom and FreeStyle Libre exports, from a file or a byte stream. Each row becomes a `CgmReading` with a `CgmSource` and optional `TrendArrow`.
- **Validation and ordering**: `validate_cgm_readings` flags implausible values, and `sort_readings` puts a feed into time order before analysis.
- **Resampling and gap filling** (`resampling`): `resample_time_series` moves readings onto a regular grid using an `InterpolationMethod` set in `ResampleConfig`. `fill_gaps` fills gaps up to `max_gap_minutes` using a `GapFillMethod`.
- **Rolling statistics**: `rolling_statistics` computes windowed statistics selected by `RollingStat`.
- **Glycemic variability** (`variability`): `calculate_glycemic_variability` returns a `GlycemicVariability` summary. Time-in-range results come as `TimeInRangeMetrics`, with `TimeInRange`, `TimeBelowRangeMetrics` and `TimeAboveRangeMetrics` breakdowns.
- **Rhythm analysis**: `analyze_rhythms` finds recurring daily patterns and reports them as `RhythmAnalysis`, with `Anomaly` entries graded by `AnomalySeverity`.
- **Longitudinal trajectories** (`trajectory`): `analyze_trajectory` summarizes an `OrganFunctionTrajectory` as a `TrajectoryAnalysis` of `TrajectoryPoint` values and `TrendDirection`. `compare_trajectories` produces a `TrajectoryComparison` between two series.
- **Storage** (`storage`): `insert_cgm_reading` and `insert_cgm_readings` write readings to Postgres.

## Data classes

| Constant | Value | Use |
|---|---|---|
| `DATA_CLASS_CGM_CONTINUOUS` | `cgm_continuous` | Raw and derived CGM data |
| `DATA_CLASS_ORGAN_FUNCTION_TRAJECTORY` | `organ_function_trajectory` | Longitudinal organ-function series |

Access to these classes is controlled by [`tpt-soma-capability`](../tpt-soma-capability/README.md) tokens and logged by [`tpt-soma-audit`](../tpt-soma-audit/README.md).

## Errors

`ChronosError` separates resampling, gap-filling and variability failures from database, capability, audit and core errors, plus `NotFound` and `InvalidInput`. Errors from the capability and audit layers are sorted into their variants by matching the message text, so callers can tell an authorization failure from a data problem.

## Crate layout

| Module | Responsibility |
|---|---|
| `cgm` | `CgmReading`, `CgmSource`, `TrendArrow`, validation and sorting |
| `dexcom`, `libre` | Vendor CSV and stream parsers |
| `resampling` | `ResampleConfig`, `InterpolationMethod`, `GapFillMethod`, `resample_time_series`, `fill_gaps`, `rolling_statistics` |
| `variability` | `GlycemicVariability`, time-in-range metrics, `analyze_rhythms` |
| `trajectory` | `OrganFunctionTrajectory`, `analyze_trajectory`, `compare_trajectories` |
| `storage` | Postgres persistence for CGM readings |

## Running the tests

```sh
cargo test -p tpt-soma-chronos
```

`tests/golden_file_tests.rs` checks the parsers and metrics against recorded export files. The coverage floor is 65% (see [coverage targets](../../docs/testing/coverage-targets.md)).

## Related crates

- [`tpt-soma-organon`](../tpt-soma-organon/README.md): uses trajectories for organ-function analysis
- [`tpt-soma-harmonize`](../tpt-soma-harmonize/README.md): maps readings to ontology codes
- [`tpt-soma-api`](../tpt-soma-api/README.md): exposes CGM routes and ingest

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
