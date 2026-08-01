# ADR-002: Custom Capability-Based Access Control (CBAC)

## Status

Accepted

## Context

The spec calls for proof-native security: a researcher must present an unforgeable cryptographic capability token to access a specific data class and cohort. The system cannot execute a query it is not authorized to perform.

Adjacent sibling projects were evaluated:

- **`tpt-archon`** — provides kernel/IPC-scoped capabilities (memory page access, cache control). Its capability model is fundamentally about OS-level resource partitioning, not researcher-to-data-class authorization.
- **Keystone's Mirror component** — provides agent-action tracing for operational observability, not HIPAA/GDPR-style compliance audit trails.

Neither semantic fit matches the researcher-facing data access control required by `tpt-soma`.

## Decision

Build a custom capability-token cryptosystem in `tpt-soma-capability`:

- **Token format**: Ed25519-signed struct containing `subject`, `resource_class`, `cohort_scope`, `action`, `expiry`, and `nonce`.
- **Attenuation**: HMAC-chained caveats allow deriving a narrower child token without re-signing by the root key. A child token cannot exceed its parent's scope (enforced by HMAC comparison).
- **Root key lifecycle**: Development = local keyfile; abstracted behind a trait so production can later swap in KMS/HSM.
- **Revocation**: Revocation list keyed by nonce; short TTL as defense-in-depth.

The capability token is the single authorization primitive across all query endpoints in all domain crates.

## Consequences

- **Positive**: Purpose-built for researcher/data-class scoped access; no semantic mismatch.
- **Positive**: Independent of sibling project roadcycles; `tpt-archon` and Keystone remain free to evolve without blocking `tpt-soma`'s security stack.
- **Trade-off**: Additional build surface for crypto. Mitigated by using well-audited primitives (Ed25519, HMAC-SHA256) and keeping the token format minimal.
- **Trade-off**: No out-of-the-box SSO/OAuth in Phase 1; researchers are issued tokens via CLI/admin script. SSO is deferred to Phase 4.
