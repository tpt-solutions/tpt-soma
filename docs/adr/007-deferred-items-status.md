# ADR 007 — Status & implementation plan for deferred Phase 3/4 items

- Status: accepted
- Date: 2026-08-08
- Context: TODO.md marks several Phase 3/4 items `deferred` by explicit
  scope decision (see the "Descoping decisions" table and the Phase 3/4
  progress notes). This ADR records their current status, the rationale for
  keeping them deferred, and the concrete next-step plan. It is the single
  pointer the TODO references for every still-open deferred item.

## 1. Items already satisfied by existing code (reconcile TODO)

| TODO item | Where | Note |
|---|---|---|
| DP extended to simulation aggregate exports (Phase 3, ~line 177) | `crates/tpt-soma-api/src/server.rs:794` `simulation_aggregate_count` | routes through `DifferentialPrivacyService::cohort_aggregate_export` with the `"simulation"` domain; epsilon budget + audit hook apply. |
| Flight RPC extended for simulation queries (Phase 3, ~line 181) | `crates/tpt-soma-api/src/flight.rs` | `simulation` descriptor + `simulation_output` data class wired in `get_flight_info` and `do_get` (added this pass). |
| Simulation configuration UI + results viz (Phase 3, ~line 180) | `frontend/src/SimulationPanel.tsx` | runs `/api/v1/simulate`, fetches `/api/v1/simulations/:run_id`, renders per-series `TrajectoryChart`. |
| Federated encrypted result/weight return protocol (Phase 4, ~line 226) | `crates/tpt-soma-core/src/federated.rs` | capability-scoped, HMAC-authenticated `FederatedResultEnvelope`; payload encryption is the site's responsibility. |
| Wasm sandbox host API surface (Phase 4, ~217/218) | `crates/tpt-soma-sandbox/src/lib.rs` | `ResearcherCompute` trait + `execute_capability_scoped` gated by the same capability tokens; actual isolation to reuse Keystone `wasmtime` UDFs. |

## 2. Genuinely deferred items — status & plan

### 2.1 Differentiable physiology: PyTorch/JAX bridge (Phase 3, ~line 167)
- **Status:** deferred. Calibration MVP fits parameters with native Rust
  finite-difference gradients (`crates/tpt-soma-simulacrum/src/calibration.rs`).
- **Decision:** no PyTorch/JAX dependency added. The numerical-gradient seam
  (`calibrate`) is the integration point if a differentiable backend is later
  required for high-dimensional fits.
- **Plan:** add `simulacrum::autodiff` only if a concrete research use case
  needs gradient-based fitting beyond the ODE/PDE models; prefer binding a
  Rust autodiff crate (e.g. `nera`/`reverse-mode`) over pulling a Python
  runtime into the API process.

### 2.2 Simulation outputs via Chronos / Plexus (Phase 3, ~line 173)
- **Status:** **implemented.** Relational `simulation_outputs` remains the
  authoritative store (migration `20240101000006_phase3_simulacrum.sql`). A new
  Chronos-style `simulation_series` table (migration
  `20240101000008_simulation_chronos_mirror.sql`) is populated by
  `simulacrum::storage::mirror_outputs_to_chronos`, and
  `simulacrum::storage::mirror_run_to_plexus` writes `cross_talk` edges from a
  run to the OSG nodes it touched.
- **Caveat:** `mirror_run_to_plexus` calls Keystone's Plexus `create_edge`
  over the Postgres wire; the assumed `(source, target, edge_type, properties)`
  signature must be validated against the deployed Plexus extension (the
  project deliberately does not pin a Plexus SDK). Both functions are exercised
  by `#[ignore]` integration tests gated on `TEST_DATABASE_URL`.

### 2.3 Phase 4 domain sub-modules: Oncology / Longevity & Aging /
###     Cardiovascular / autoimmune / infectious (Phase 4, ~202–204)
- **Status:** deferred. `tpt-soma-pathos` ships metabolic & endocrine only.
- **Plan:** each becomes its own `pathos::*` module building on the OSG and the
  cross-talk solver; scope narrowly (one validated method per module) per the
  established orchestrate-don't-reimplement pattern.

### 2.4 clinica: EHR/FHIR hardening + clinical trial design (Phase 4, ~207–208)
- **Status:** deferred. Full resource coverage and trial logic not built;
  `clinical_trial_cohorts` tables scaffolded, biomarker table + data class
  seeded (migration 07).
- **Plan:** extend the Phase 2 FHIR subset (`tpt-soma-organon`) as needed;
  cohort-discovery/recruitment/ae-tracking logic layered on existing
  `cohort_membership` + `clinical_trial_cohorts`.

### 2.5 Wasm sandbox execution backend (Phase 4, ~217–218)
- **Status:** **implemented.** `tpt-soma-sandbox` now provides a real WASM
  execution backend behind the `wasmtime-backend` Cargo feature (gated so the
  default CI build stays fast). `WasmtimeCompute` implements the
  `ResearcherCompute` trait behind the same `execute_capability_scoped` gate,
  following a small, toolchain-agnostic guest ABI (fixed input/output memory
  offsets, `run(input_len) -> result_len`). Round-trip tests run an embedded
  WAT guest to prove real execution.
- **Note:** the plan mentioned reusing Keystone's `wasmtime` UDF sandbox; the
  in-repo `wasmtime` backend is the equivalent self-contained seam and is
  capability-gated exactly like the host API expects.

### 2.6 3D visualization / interactive systemic query builder (Phase 4, ~221–222)
- **Status:** deferred.
- **Plan:** map OSG macro nodes to a deck.gl/three.js whole-body renderer
  reading the existing OSG topology; query builder drives the cross-talk solver.

### 2.7 Federated pilot w/ partner site + audit reconciliation (Phase 4, ~227, 232)
- **Status:** deploy reuse done (~225); **cross-site ledger reconciliation
  implemented** in `tpt-soma-core::federated`
  (`LedgerConsistencyProof` + `prove_ledger_consistency` /
  `verify_ledger_consistency`) binds a `FederatedResultEnvelope` to the central
  audit ledger's tail `row_hash` under the scope key. The single-site pilot
  remains an operational/onboarding task (run the existing Helm chart at a
  partner site), not a code change here.
- **Plan:** run the existing Helm chart at one partner site; the consistency-proof
  path is now in place for that pilot to use.

### 2.8 Graph-traversal-scoped capability access + cross-domain DP (Phase 4, ~230–231)
- **Status:** **implemented.** `CapabilityToken` now carries an optional
  `graph_scope: Option<Vec<String>>` (signed into the token payload); the API's
  `graph_scope_allows(token, entity_id)` enforces per-node/edge traversal scope
  at graph query endpoints (unit-tested in `auth.rs`). Cross-domain DP exports
  are implemented end to end: `DifferentialPrivacyService::cross_domain_aggregate_export`
  releases a single noisy count over multiple Phase 1–4 data classes, and
  `POST /api/v1/cohorts/:cohort_id/aggregate/cross-domain` (wired through the
  Phase 0 DP module + capability/audit choke-points) exposes it.

### 2.9 End-to-end systemic query test + full-stack load test (Phase 4, ~238–239)
- **Status:** deferred (need populated OSG + cross-talk run; target hardware).
- **Plan:** extend `crates/tpt-soma-api/tests/e2e_flight.rs` and the
  `scripts/loadtest` suite once 2.2/2.3 land.

## 3. Consequences
- The roadmap's security/compliance requirements are never dropped by
  deferring domain breadth; every deferred path still must enter through the
  Phase 0 capability/audit/DP choke-points.
- Deferred items remain explicitly tracked here rather than silently dropped,
  so future passes have a single source of truth.
