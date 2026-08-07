# Audit Ledger

## Summary

`tpt-soma` maintains an append-only, tamper-evident audit ledger recording who
accessed what data, when, and with what outcome. It is a compliance artifact
for HIPAA/GDPR-style accountability and for demonstrating data-use
transparency to an IRB.

## Ledger structure

The ledger is the Keystone `audit_ledger` table:

| Column | Meaning |
|---|---|
| `id` | UUID event identifier. |
| `actor` | The token subject (researcher identity). |
| `resource_class` | Data class accessed (e.g. `genomic_variant`, `clinical_observation`). |
| `action` | Action performed (`read`, `write`, `export`, `spend`). |
| `cohort_scope` | Cohort IDs the request touched. |
| `timestamp` | UTC event time. |
| `query_fingerprint` | SHA-256 of method + path + query string. |
| `outcome` | `success` / `failure` / `pending`. |
| `prev_row_hash` | Hash of the previous ledger row. |
| `row_hash` | Hash of this row's content plus the previous hash. |

## Hash chaining

Each appended event is hashed as:

```
row_hash = SHA-256( prev_row_hash || canonical_json(event_payload) )
```

where `event_payload` includes the event's own fields (id, actor, resource
class, action, cohort scope, timestamp, query fingerprint, outcome) but **no
raw PHI values**. Tampering with any historical row breaks the chain and is
detectable by the chain-integrity verification job.

## Single choke-point write path

Audit writes happen inside `capability_middleware` in `tpt-soma-api`, which
wraps the entire router. Every authenticated request therefore produces an
audit event without per-endpoint audit code. DP budget spends are recorded
through the same ledger via `record_dp_budget_spend`.

This design means new endpoints and new data classes get audit coverage
automatically as long as they are registered inside the router.

## Verification and reporting

- **Integrity:** `integrity.rs::verify_chain` walks the whole chain and
  recomputes hashes; exposed via `audit-cli verify-chain`. Any mismatch
  (tampered hash, tampered prev_hash, out-of-order insertion) is reported.
- **Compliance reports:** `audit-cli cohort-access --cohort … --from … --to …`
  produces a per-cohort access report for a time window.

## Relevant code

- `crates/tpt-soma-audit/src/ledger.rs` — append, tail_hash, verify_chain
- `crates/tpt-soma-audit/src/integrity.rs` — chain verification
- `crates/tpt-soma-audit/src/bin/*` — `audit-cli` commands
- `crates/tpt-soma-api/src/auth.rs` — `capability_middleware` audit write
- `crates/tpt-soma-core/migrations/20240101000001_init_audit.sql` — schema
