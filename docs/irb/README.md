# IRB / Ethics Documentation Package

This package documents the technical and procedural controls implemented in
`tpt-soma` that are relevant to institutional review board (IRB) and ethics
committee submissions for research using the platform. It is a **non-code
deliverable** describing the mechanisms the software provides; it is not legal
or regulatory advice.

## Documents

| Document | Purpose |
|---|---|
| [`capability-based-access-control.md`](capability-based-access-control.md) | How researcher access is tokenized, scoped, attenuated, and revoked (CBAC). |
| [`differential-privacy.md`](differential-privacy.md) | How cohort-level aggregate exports are noise-injected and epsilon-budgeted. |
| [`audit-ledger.md`](audit-ledger.md) | The append-only, hash-chained compliance audit trail. |
| [`data-classification.md`](data-classification.md) | Data inventory, sensitivity classification, and data flow. |
| [`research-protocol.md`](research-protocol.md) | Intended research use, data minimization, and participant protections. |

## How the documents map to system features

- **CBAC** — `tpt-soma-capability` crate: Ed25519-signed capability tokens
  (subject, resource class, cohort scope, action, expiry, nonce), revocation
  list, attenuation rules. Enforced by `capability_middleware` in the API layer.
- **DP** — `tpt-soma-core::dp` module: Laplace mechanism, per-cohort epsilon
  budget, single enforcement code path for cohort aggregate exports, spend
  recorded through the audit ledger.
- **Audit** — `tpt-soma-audit` crate: append-only `audit_ledger` table with
  `row_hash = H(prev_row_hash || payload)` chaining, chain-integrity
  verification job, compliance report generator.
- **Data classification** — `data_class_registry` table + the registry in
  `tpt-soma-capability`, sensitivity labels from `public` to `restricted`.

## Revision guidance

- Update this package when the security stack, data inventory, or research
  scope changes.
- Keep in sync with the threat model in [`../security/threat-model.md`](../security/threat-model.md).
- These documents describe intent and mechanism; evidence of operation
  (real audit records, budget spend, access logs) is produced by the system
  itself and is the stronger artifact for a submission.
