# tpt-soma-organon

Organ function, FHIR/CSV ingestion, imaging metadata, and clinical calculators.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-organon` models the health of individual organs. It ingests clinical observations (FHIR resources and CSV exports), organ imaging metadata, and computes standard clinical scores. It also represents organs as a graph, so that a dysfunction in one system can be traced to others.

## Features

- **Clinical observations** (`ingestion`, `loinc`): parses FHIR `Observation` resources (`parse_fhir_observation`) and organ-function CSV files (`parse_organ_function_csv`, `csv_to_clinical_observation`). Observations are keyed by LOINC code, and the `loinc` module holds the constants for the analytes the platform uses (for example `CREATININE`, `ALT`, `HBA1C`, `EJECTION_FRACTION`, `FEV1`, `TROPONIN_I`). Each observation carries a `ReferenceStatus`.
- **Imaging metadata** (`imaging`): `parse_dicom_metadata` and `validate_dicom_metadata` read DICOM headers, not pixel data. `generate_imaging_object_key` builds the object-store key, and `ImagingModality` and `Laterality` classify the study.
- **Clinical calculators** (`calculator`): `gfr_ckd_epi_2021` (eGFR), `ejection_fraction`, `ast_alt_ratio`, and the `LiverInterpretation` and `PulmonaryInterpretation` enums classify results. Each calculator's inputs and output are documented on the function.
- **Organ-system graph** (`organ_system`): `OrganSystemGraph` holds organs as nodes and couplings as edges, typed by `CouplingType` and graded by `EvidenceLevel`. `find_paths` and `get_neighbors` navigate it, `cascade_risk` estimates downstream risk, and `simulate_cascade` steps a failure through it.
- **Storage** (`storage`): `insert_clinical_observation`, `insert_clinical_observations`, `insert_organ_imaging_record` and `insert_fhir_resource_payload` persist records. Graph data is saved with `to_file` and reloaded with `from_file`.

Code mapping is provided by [`tpt-soma-harmonize`](../tpt-soma-harmonize/README.md), and `OrganonError` wraps its errors in a `Harmonization` variant.

## Data classes

| Constant | Value | Use |
|---|---|---|
| `DATA_CLASS_CLINICAL_OBSERVATION` | `clinical_observation` | Lab, vital and FHIR observations |
| `DATA_CLASS_ORGAN_IMAGING` | `organ_imaging` | Imaging study metadata |

## Crate layout

| Module | Responsibility |
|---|---|
| `calculator` | Clinical score functions and interpretation enums |
| `imaging` | DICOM metadata parsing and object keys |
| `ingestion` | FHIR and CSV parsers |
| `loinc` | LOINC code constants |
| `organ_system` | Organ-system graph, paths, cascades |
| `storage` | Postgres persistence |

Errors are reported as `OrganonError`. Errors from the capability, audit, Chronos and harmonization layers are mapped into their own variants.

## Configuration files

`config/reference_ranges.json` holds the reference ranges as data, so they can be reviewed without a code change.

## Running the tests

```sh
cargo test -p tpt-soma-organon
```

`tests/golden_file_tests.rs` checks parsers and calculators against recorded outputs. The coverage floor is 65% (see [coverage targets](../../docs/testing/coverage-targets.md)).

## Related crates

- [`tpt-soma-chronos`](../tpt-soma-chronos/README.md): longitudinal trajectories for organ function
- [`tpt-soma-harmonize`](../tpt-soma-harmonize/README.md): LOINC and SNOMED mapping
- [`tpt-soma-pathos`](../tpt-soma-pathos/README.md): disease models that use these observations

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
