# tpt-soma-core

Keystone connection pooling, SQL migrations, query helpers, and the differential-privacy foundation.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-core` is the shared base layer. Every other domain crate depends on it for the Postgres pool, the schema migrations, typed query helpers, object storage, the differential-privacy engine, and the federated result envelope. It holds no domain logic of its own.

## Features

- **Connection pool** (`connection`): `create_pool` builds the shared `PgPool` against Keystone Postgres, and `run_migrations` applies the schema. Errors are reported as `CoreError`.
- **Migrations** (`migrations`): versioned SQL under `migrations/`, applied in order. The ten migrations cover the audit ledger, the Phase 1 schema (subjects, samples, variants, expression), scRNA UMAP, the Plexus graph, Phase 2 organ-function and Chronos tables, Phase 3 simulation, Phase 4 organ-system graph, simulation-to-Chronos mirroring, device state, and clinical trial results.
- **Query helpers** (`query`): typed reads such as `get_cgm_readings_by_subject`, `get_clinical_observations_by_subject_and_loinc`, `get_variants_by_sample`, `get_expression_by_gene`, `get_umap_by_cluster`, `get_cohort_samples`, `join_variant_expression`, `get_cross_phase_subject_summary`, and graph traversal with `graph_neighbors` and `graph_bfs`.
- **Record types**: `CgmReadingRecord`, `ClinicalObservationRecord`, `VariantRecord`, `ExpressionRecord`, `UmapRecord`, `OrganImagingRecord`, `SampleRecord`, `VariantExpressionJoin` and others.
- **Object storage** (`store`): `ObjectStoreClient` uploads raw files to MinIO or S3-compatible storage. `upload_with_checksum` checks the MD5 of the bytes against the checksum supplied with the upload. `upload_to_quarantine` stores input that failed to parse in a separate `<bucket>-quarantine` bucket. `ObjectStoreClient::from_env` reads `MINIO_*` variables.
- **Differential privacy** (`dp`): the Laplace mechanism (`laplace_noise`, `noisy_count`), per-cohort epsilon budgets (`spend_budget`, `remaining_budget`, `BudgetError`), and `DifferentialPrivacyService`, which pairs the budget with an audit hook (`BudgetAuditHook`). Exports are produced by `cohort_aggregate_export` and `cross_domain_aggregate_export`, which spend budget before releasing a value.
- **Federated envelopes** (`federated`): `FederatedResultEnvelope` returns results from a site with an HMAC over the payload (`sign_envelope`, `verify_envelope`). Payload encryption is the site's responsibility, as recorded in [ADR 007](../../docs/adr/007-deferred-items-status.md). `prove_ledger_consistency` builds a `LedgerConsistencyProof`, and `verify_ledger_consistency` checks one.
- **Test helpers** (`test_helpers`): `test_pool` gives tests a Postgres pool from `DATABASE_URL`.

## Differential privacy in practice

Aggregate routes in [`tpt-soma-api`](../tpt-soma-api/README.md) call the DP service rather than querying counts directly. The cohort's epsilon budget is set by `DP_EPSILON`. Once a cohort has spent its budget, further releases for that cohort fail with `BudgetError`. Each spend is recorded through the audit hook, so the budget history can be reconstructed from the ledger.

## Configuration

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | Postgres connection string |
| `MINIO_ENDPOINT`, `MINIO_BUCKET`, `MINIO_ACCESS_KEY`, `MINIO_SECRET_KEY` | Object store (defaults point at a local MinIO) |

## Crate layout

| Module | Responsibility |
|---|---|
| `connection` | `PgPool`, `create_pool`, `run_migrations`, `CoreError` |
| `migrations` | Migration runner for the bundled SQL |
| `query` | Typed read helpers and graph traversal |
| `store` | `ObjectStoreClient`, checksummed and quarantined uploads |
| `dp` | Laplace mechanism, budgets, `DifferentialPrivacyService` |
| `federated` | Result envelopes and ledger consistency proofs |
| `test_helpers` | `test_pool` for integration tests |

## Running the tests

```sh
cargo test -p tpt-soma-core
```

`tests/dp_tests.rs` covers budget exhaustion and noise behavior. `tests/cross_phase_test.rs` and `tests/load_test.rs` need a running Postgres. The DP module targets 90%+ coverage and the crate targets 70% (see [coverage targets](../../docs/testing/coverage-targets.md)).

## Related crates

- [`tpt-soma-audit`](../tpt-soma-audit/README.md): the ledger that the budget hook writes to
- [`tpt-soma-ingest`](../tpt-soma-ingest/README.md): uploads through the object store
- Every other domain crate builds on these types

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
