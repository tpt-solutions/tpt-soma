# ADR-003: Custom Hash-Chained Audit Ledger

## Status

Accepted

## Context

Compliance with HIPAA/GDPR requires an immutable, tamper-evident audit trail of every data access and pipeline execution. Keystone includes a Mirror component for agent-action tracing, but its semantics are operational observability — it tracks internal agent actions, not researcher-initiated queries against protected data classes.

## Decision

Build a custom append-only audit ledger in `tpt-soma-audit`:

- **Table**: `audit_ledger` with columns for `id`, `actor`, `resource_class`, `action`, `cohort_scope`, `timestamp`, `query_fingerprint`, `outcome`, `prev_row_hash`, `row_hash`.
- **Hash chain**: Each row's `row_hash = H(prev_row_hash || event_payload)`. The first row's `prev_row_hash` is `NULL` or a fixed genesis value.
- **Single write path**: Audit logging happens inside the capability-verification middleware, not scattered per-endpoint. Every query passes through the same `tpt-soma-audit` write path.
- **No raw PHI values**: The ledger never stores actual biomarker values, genomic sequences, or demographic fields. It stores metadata sufficient to reconstruct *who accessed what*.

Additionally:

- **Chain-integrity verification job** (scheduled): recomputes the hash chain and alerts on mismatch.
- **Compliance report generator** (CLI): shows all access to a cohort within a date range.

## Consequences

- **Positive**: Tamper-evident without requiring external SIEM tooling; standalone proof of compliance.
- **Positive**: Single write path means no gaps between endpoint implementations.
- **Trade-off**: Append-only table will grow unbounded. Mitigated by time-based partitioning (e.g., monthly partitions) and archiving older partitions to cold storage.
- **Trade-off**: Hash chain verification is O(N) over the full table. Mitigated by running it as a scheduled job, not synchronously.
