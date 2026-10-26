# tpt-soma-api

HTTP and Arrow Flight API server with capability, audit and differential-privacy enforcement.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-api` is the front door to the platform. Every request to a data-bearing route must carry a signed capability token; the server checks the token, writes an audit event, and (for aggregate exports) spends differential-privacy budget before any result leaves the process. Nothing in the domain crates is reachable except through these checks.

## Features

- **REST API (axum)** under `/api/v1` for CGM and organ-function data, variants, expression and UMAP results, organ-system graphs, simulations, systemic queries, clinical trials and cohort aggregates.
- **Ingestion endpoints** for CGM, imaging, organ CSV, FHIR observations, VCF and h5ad uploads, plus a device-state write path.
- **Arrow Flight gRPC service** (`flight.rs`) for columnar `GetFlightInfo` and `DoGet`/`DoPut` exchange with `pyarrow.flight` and other standard clients. It is built on the in-house [`tpt-soma-flight`](../tpt-soma-flight/README.md) stack.
- **Capability middleware** (`auth.rs`): bearer-token authentication, per-route required capability (`required_capability_for`, `enforce_route_policy`), scope checks against the token's resource class, cohort scope and action, and graph-scope checks for organ-system queries.
- **Audit on every guarded call**: `audit_event` writes to the hash-chained [`tpt-soma-audit`](../tpt-soma-audit/README.md) ledger, including a fingerprint of the query (`compute_query_fingerprint`) and never raw values.
- **Differential privacy on aggregate routes**: cohort and simulation counts go through `DifferentialPrivacyService` in [`tpt-soma-core`](../tpt-soma-core/README.md), which spends the cohort's epsilon budget and records the spend (`record_dp_budget_spend`, `record_dp_budget_spend_with_actor`).
- **Prometheus metrics** (`metrics.rs`): an in-house registry of monotonic counters, rendered in the Prometheus text format at `/metrics`.
- **Secrets loading** (`secrets.rs`): loads the Ed25519 capability root key from a file path.
- **Device ingest** (`device_ingest.rs`): validates device-state rows (required columns, finite values, non-negative timestamps, known pathway states) and caps batches at `MAX_ROWS_PER_BATCH`.

## Binaries

| Binary | Source | Purpose |
|---|---|---|
| `tpt-soma-api` | `src/main.rs` | REST API server, default listen `0.0.0.0:8080` |
| `tpt-soma-flight` | `src/flight_main.rs` | Arrow Flight gRPC server, default listen `0.0.0.0:8815` |
| `tpt-soma-admin` | `src/bin/admin.rs` | Operator CLI: `gen-key`, `issue` and `revoke` capability tokens |

### Configuration

Each option can be set as a flag or an environment variable.

| Variable | Used by | Default | Meaning |
|---|---|---|---|
| `DATABASE_URL` | api, flight | none | Postgres connection string (Keystone) |
| `LISTEN_ADDR` | api | `0.0.0.0:8080` | REST listen address |
| `FLIGHT_LISTEN_ADDR` | flight | `0.0.0.0:8815` | Flight gRPC listen address |
| `METRICS_ADDR` | api | `0.0.0.0:9090` | Prometheus metrics listen address |
| `CAPABILITY_ROOT_KEY_PATH` | api, flight | none | Path to the Ed25519 root signing key |
| `DP_EPSILON` | api | `1.0` | Per-cohort differential-privacy epsilon budget |
| `MINIO_ENDPOINT`, `MINIO_BUCKET`, `MINIO_ACCESS_KEY`, `MINIO_SECRET_KEY` | api | local dev defaults | Raw-object store for uploads |

The admin CLI defaults to `./dev-keys` for key material. Do not use the defaults in production.

## REST routes

Routes are grouped by domain. All are under `/api/v1` unless noted.

- **Chronos and organ function:** `cgm/:subject_id`, `cgm/:subject_id/variability`, `clinical-observations/:subject_id`, `clinical-observations/:subject_id/:loinc_code/trajectory`, `organ-imaging/:subject_id`, `subjects/:patient_id/cross-phase-summary`
- **Genomics and single-cell:** `variants/:sample_id`, `variants/:sample_id/:variant_id`, `expression/:sample_id`, `expression/gene/:gene_id`, `umap/:sample_id`, `umap/:sample_id/cluster/:cluster`, `join/variant-expression`
- **Cohorts and aggregates:** `cohorts/:cohort_id/samples`, `cohorts/:cohort_id/aggregate/count`, `cohorts/:cohort_id/aggregate/cross-domain`
- **Simulation and graphs:** `simulate`, `simulations/:run_id`, `simulations/:run_id/aggregate/count`, `systemic/query`, `organ-system-graph`, `organ-system-graph/cascade`
- **Clinical trials:** `trials`, `trials/:trial_id/screen`, `trials/:trial_id/adverse-events`, `trials/:trial_id/adverse-events/summary`
- **Ingestion:** `ingest/cgm`, `ingest/imaging`, `ingest/organ-csv`, `ingest/fhir-observation`, `ingest/vcf`, `ingest/h5ad`
- **Operations:** `GET /health`, `GET /metrics`

Guarded routes are mapped to their required capability in `required_capability_for`. Add any new guarded route there.

## Security model

- Authentication is by bearer capability token. Tokens are verified against the root key and checked for expiry and revocation before the handler runs.
- A token authorizes one resource class, one action and a cohort scope. The middleware rejects a request whose route, class or cohort falls outside that scope.
- The middleware writes an audit row for each request that passes authentication, with its outcome taken from the response status. Audit rows hold metadata only, never biomarker values, sequences or demographics.
- Aggregate and systemic results pass through differential-privacy release. A cohort whose epsilon budget is exhausted gets no further releases.

See [ADR 002](../../docs/adr/002-custom-capability-tokens.md) and [ADR 003](../../docs/adr/003-custom-audit-ledger.md).

## Crate layout

| Module | Responsibility |
|---|---|
| `server` | `ApiServer`, router construction and the REST handlers |
| `auth` | `AuthState`, `capability_middleware`, `authenticate_bearer`, route policy |
| `flight` | `FlightServer`, the Flight `GetFlightInfo`, `DoGet` and `DoPut` handlers |
| `device_ingest` | Device-state validation and batch limits |
| `metrics` | Counters and the Prometheus text renderer |
| `secrets` | Root-key loading |
| `error` | `ApiError`, `AuthError`, `FlightError`, `IngestError` |

## Running the tests

```sh
cargo test -p tpt-soma-api
```

Middleware tests in `capability_middleware_in_memory.rs` run without a database. The end-to-end tests (`e2e_flight.rs`, `e2e_real_phi_pilot.rs`) need a running Postgres configured through `DATABASE_URL`. The coverage floor for this crate is 60% (see [coverage targets](../../docs/testing/coverage-targets.md)).

## Related crates

- [`tpt-soma-capability`](../tpt-soma-capability/README.md): token format, signing, attenuation, revocation
- [`tpt-soma-audit`](../tpt-soma-audit/README.md): hash-chained audit ledger
- [`tpt-soma-core`](../tpt-soma-core/README.md): connection pool, migrations, differential privacy
- [`tpt-soma-flight`](../tpt-soma-flight/README.md): columnar batches and Flight protocol
- [`tpt-soma-ingest`](../tpt-soma-ingest/README.md), [`tpt-soma-chronos`](../tpt-soma-chronos/README.md), [`tpt-soma-organon`](../tpt-soma-organon/README.md), [`tpt-soma-simulacrum`](../tpt-soma-simulacrum/README.md), [`tpt-soma-clinica`](../tpt-soma-clinica/README.md): domain logic behind the routes

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
