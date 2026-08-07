# Internal Threat Model Review

Status: **Review complete (2026-08)** — the STRIDE-per-asset walkthrough below
was performed by reading the source. Findings are tracked with their fix status.
Open findings (TM-03, TM-06, TM-09) and the sign-off gate at the bottom must be
closed or accepted **before any real patient sample onboarding** (Phase 1 TODO
item, Phase 2 real-PHI pilot gate). The review itself is done; this file is the
deliverable for the "internal threat-model review" TODO item.

## Scope & approach

Assets: raw omics/imaging blobs (MinIO), normalized data (Keystone relational +
Plexus graph + Chronos time-series), capability signing key, audit ledger,
API + Flight services, frontend, deployment secrets.

Approach: STRIDE-per-asset walkthrough of the HTTP API, Arrow Flight service,
capability cryptosystem, audit ledger, and deployment configuration, cross
checked against the source (this review found concrete, verifiable issues by
reading the code, not just reasoning about it).

## Findings

### TM-01 — Capability middleware does not enforce resource class / action / cohort scope

`capability_middleware` (`crates/tpt-soma-api/src/auth.rs`) verifies signature,
expiry, and revocation, but the resource-class registry check is commented out
(`auth.rs:117-118`) and there is **no enforcement** of `action` or
`cohort_scope` against the requested route/resource. `AuthError::InsufficientScope`
(→ HTTP 403) is never raised. A token issued for `read` on `genomic_variant`
would currently be accepted for any route, including write/ingest routes and
other data classes.

**Risk:** High. **Fix:** implement route→required (resource class, action, cohort)
policy mapping in the middleware. **Status:** **FIXED (2026-08)** — `auth.rs`
now enforces a route-policy table (`required_capability_for` /
`enforce_route_policy`): wrong resource class, insufficient action, and
out-of-cohort requests are rejected with 403 `InsufficientScope`. Covered by
unit tests in `auth.rs` and the in-memory middleware suite.

### TM-02 — Arrow Flight service has no authentication

`flight.rs` `handshake` returns an empty response and `do_get` performs no
capability check. Any client that can reach the Flight port (8815) can run
`variants`, `expression`, `umap`, `clinical_observations`, and `cgm` queries
against the database with no token.

**Risk:** Critical (public exposure) / High (network-isolated). **Fix:** require
a capability token — either via the handshake token exchange or a header
carried on `get_flight_info`/`do_get` calls — and validate it before executing
queries. **Status:** **FIXED (2026-08)** — `flight.rs` now validates a
`Bearer` capability token on `get_flight_info` and `do_get` (via
`authorize_flight_call`), enforcing a `read`-level action and the data type's
resource class. The Flight binary reads `CAPABILITY_ROOT_KEY_PATH`. Covered by
`e2e_flight.rs` (DB-backed) and in-process tests.

### TM-03 — Revocation list is in-memory only

`RevocationList` (`crates/tpt-soma-capability/src/revocation.rs`) is a
`HashSet` behind a lock. Revocations are lost on restart; there is no
persistence to Keystone and no way for a second `tpt-soma-api` replica to share
the list. A revoked token remains valid after an API restart.

**Risk:** Medium-High (depends on token lifetime being short). **Fix:**
persist revocations (e.g. a Keystone table consulted at verify time), or rely
on short expiry plus documented restart-revoke procedure. **Status:** open.

### TM-04 — Admin tools can emit invalid tokens / cannot revoke against a live server

- `tpt-soma-api` `bin/admin.rs` (the `tpt-soma-admin` binary) is a stub: its
  `Issue` command emits a token with empty signature and `expiry: 0`, which the
  server will always reject (401). Operators using the wrong binary get silent
  breakage.
- `tpt-soma-admin Revoke` and the `admin Revoke` command only mutate a fresh
  in-process `RevocationList`, not the running server's list.

**Risk:** Medium (operational). **Fix:** delete the stub binary or make it call
the same code path as `src/bin/admin.rs`; add a real revoke endpoint/CLI that
updates the server's revocation list (or the persisted store from TM-03).
**Status:** stub `tpt-soma-admin` binary **removed (2026-08)** and the signing
`src/bin/admin.rs` CLI **wired in** as a proper `[[bin]]` target (was silently
excluded from the build by explicit `[[bin]]` autodiscovery). Live revoke
against a running server still requires the TM-03 persistence work.
**Status:** partially addressed.

### TM-05 — Token expiry flag ignored

`CapabilityToken::sign` (`token.rs:23`) overwrites `expiry` to `now + 3600`
regardless of the requested expiry. The CLI's `--expiry` option is silently
ignored. This is not a vulnerability per se but defeats the intended short/
long expiry control and can mislead operators.

**Risk:** Low-Medium. **Fix:** honor the supplied expiry (bounded by a
configured maximum). **Status:** **FIXED (2026-08)** — `CapabilityToken::sign`
now signs the expiry exactly as supplied; the admin CLI's `--expiry` is
honored. A maximum-expiry clamp is left to issuance policy (admin CLI),
not the signing primitive.

### TM-06 — Audit writes are fire-and-forget

`capability_middleware` spawns the ledger append with `let _` (errors
swallowed). A DB hiccup silently drops audit records; the compliance trail
would have gaps that the chain-integrity job cannot distinguish from a normal
empty interval.

**Risk:** Medium (compliance completeness). **Fix:** retry/queue appends,
surface append failures, and record an explicit "audit write failed" marker.
**Status:** open.

### TM-07 — Secrets in plaintext env and dev defaults

Compose and Helm ship dev defaults (`postgres`, `minioadmin`,
`bootstrapPassword: changeme`) and put `DATABASE_URL` (with password) and
MinIO keys in environment variables rather than mounted secrets. `.env.example`
documents this for dev. Acceptable for local dev; must not be used for any
deployment holding PHI without secret injection.

**Risk:** High if mis-deployed. **Fix:** enforce secret mounting in Helm for
the pilot path; add a startup check that refuses default credentials when a
`TPT_ENFORCE_SECRETS=1` flag is set. **Status:** partially addressed — API and
Flight servers now refuse to start with an ephemeral signing key when
`TPT_ENFORCE_SECRETS=1` (`crates/tpt-soma-api/src/secrets.rs`); the pilot runbook
provisions the key onto the `capability-secrets` volume. Default credentials
still accepted for local dev (documented).

### TM-08 — Health and metrics are unauthenticated (by design, note)

`/health` and `/metrics` are outside `capability_middleware`. `/metrics`
exposes request counters/process stats. Intended for observability; keep off
public ingress (internal-only network policy).

**Risk:** Low. **Fix:** none required; document network policy.

### TM-09 — Flight schema/projection drift (maintainability, not a vuln)

Flight schemas are duplicated between `get_flight_info` and the `*_to_batch`
encoders, and DB columns (`interpretation`, `trend_arrow`) are silently dropped
from the Flight projection. Risk is subtle data leaks/incorrect schemas over
time.

**Risk:** Low. **Fix:** single source of truth for schemas; include all
declared columns. **Status:** open.

## Accepted risks / notes

- DP budget is per-process (in-memory), so epsilon spend is not durable across
  restarts or replicas. For the pilot, budget persistence should be moved into
  Keystone before real aggregate usage is relied upon for compliance.

## Sign-off gate

- [ ] All `Critical`/`High` findings closed or accepted with documented
  mitigations.
- [ ] Self-pen-test checklist (see `self-pentest-checklist.md`) executed and
  results recorded.
- [ ] Security stack re-verified against the pilot cohort onboarding runbook.
