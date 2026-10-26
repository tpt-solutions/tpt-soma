# tpt-soma-audit

Append-only, hash-chained compliance audit ledger.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-audit` records who accessed which data class, cohort and action, and makes any later edit to that history detectable. Each row's hash covers the row before it, so changing, inserting or deleting a row breaks the chain at a known point. The crate is built for HIPAA and GDPR-style access accounting: it stores *who accessed what*, never the values they accessed.

## Features

- **Append-only ledger** (`AuditLedger`): `append` writes an `AuditEvent` and links it to the chain tail (`tail_hash`). There is no update or delete path.
- **Hash chain** (`integrity`): every row stores `prev_row_hash` and `row_hash = H(prev_row_hash || payload)`. `verify_chain` recomputes the chain from the start and returns a `ChainReport` that names each `ChainFailure` and its `ChainFailureKind`. `verify_chain_in_memory` runs the same check without a database.
- **Integrity job** (`run_integrity_check`): a scheduled check that runs `verify_chain` and passes any failures to an alert sink.
- **Alerting** (`alert`): `check_and_alert` turns a failing report into `IntegrityAlert` values via `alert_for`, and delivers them to any `AlertSink`. The built-in `TracingAlertSink` logs through `tracing`. Add your own sink to page an on-call system.
- **Compliance reports** (`compliance`): `ComplianceReporter` and `generate_compliance_report` list every access to a cohort inside a date range (`access_to_cohort`), as `ComplianceEntry` rows.
- **CLI** (`tpt-soma-audit`): operator commands for chain verification, continuous watching, cohort access reports and recent events.

## What is recorded

Each `AuditEvent` carries the actor, resource class, action, cohort scope, timestamp, a query fingerprint, the outcome (allowed or denied), and the chain hashes. The ledger does not store biomarker values, genomic sequences or demographic fields. The fingerprint identifies a query without containing its data.

The design routes every write through the capability middleware in [`tpt-soma-api`](../tpt-soma-api/README.md) (see ADR 003), so an endpoint does not write audit rows itself.

See [ADR 003](../../docs/adr/003-custom-audit-ledger.md) for the design.

## CLI

The binary is `tpt-soma-audit`. It reads the database from `--database-url` or `DATABASE_URL`.

| Subcommand | Purpose |
|---|---|
| `verify-chain` | Recompute the hash chain and report any failure |
| `watch` | Re-run the integrity check on an interval (default 300 seconds) |
| `cohort-access` | Report access to one cohort over a date range |
| `recent` | Show the most recent events (default 50) |

## Crate layout

| Module | Responsibility |
|---|---|
| `ledger` | `AuditLedger`, `AuditEvent`, `append`, `tail_hash`, `AuditError` |
| `integrity` | `verify_chain`, `verify_chain_in_memory`, `ChainReport`, `ChainFailure`, `IntegrityError`, `run_integrity_check` |
| `alert` | `AlertSink`, `TracingAlertSink`, `IntegrityAlert`, `alert_for`, `check_and_alert` |
| `compliance` | `ComplianceReporter`, `ComplianceEntry`, `ComplianceError`, `generate_compliance_report`, `access_to_cohort` |
| `jobs` | `run_integrity_check` and `generate_compliance_report`, the entry points for scheduled runs |

## Running the tests

```sh
cargo test -p tpt-soma-audit
```

`tests/integrity_tests.rs` covers a tampered row hash, a tampered previous-hash link, empty and single-event chains, out-of-order insertion, and an append-then-verify round trip. These tests use Postgres, so they need a running database. This is a security-critical crate with an 80% coverage floor (see [coverage targets](../../docs/testing/coverage-targets.md)).

## Related crates

- [`tpt-soma-api`](../tpt-soma-api/README.md): the only production writer of audit rows
- [`tpt-soma-core`](../tpt-soma-core/README.md): the Postgres pool and the `audit_ledger` migration
- [`tpt-soma-capability`](../tpt-soma-capability/README.md): the authorization decisions the ledger records

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
