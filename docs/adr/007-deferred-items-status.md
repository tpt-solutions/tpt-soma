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
- **Status:** **partially implemented** (code pass 2026-10-25).
  - Oncology: `oncology::simulate_gompertz_growth` (Gompertz ODE on the
    simulacrum solver, checked against the closed form) and
    `oncology::recist_response` (RECIST 1.1 categories). The earlier TME and
    immunotherapy proxies remain illustrative, not validated classifiers.
  - Longevity: `longevity::fit_gompertz_hazard` (log-linear Gompertz fit,
    recovers known parameters; mortality doubling time `ln 2 / b`). The
    phenotypic-age proxy remains illustrative.
  - Cardiovascular: `cardiovascular::windkessel_pressure_trace` (two-element
    Windkessel; mean pressure equals `R · Q̄` over whole cycles).
  - Infectious: `infectious::simulate_sir`, `basic_reproduction_number`,
    `sir_final_size` (simulated final size matches the analytic relation).
  - Autoimmune: `autoimmune::das28_crp` (published DAS28-CRP formula, worked
    example checked) and `autoimmune::classify_das28` (EULAR bands).
- **Still deferred:** epigenetic clocks and senescence tracking (longevity),
  and any validated clinical classifier. Each needs a published method and
  reference data before it counts as more than a proxy.
- **Plan:** one validated method per module, built on the OSG and the
  cross-talk solver, following the orchestrate-don't-reimplement pattern.

### 2.4 clinica: EHR/FHIR hardening + clinical trial design (Phase 4, ~207–208)
- **Status:** partially implemented (code pass 2026-10-25). `clinical_trial_cohorts`
  tables scaffolded, biomarker table + data class seeded (migration 07).
  - `trial::summarize_adverse_events`: AE counts by CTCAE-style grade 1–5,
    grade ≥3, serious, and subjects with any AE.
  - `trial::evaluate_eligibility` / `trial::screen_and_recruit`: inclusion and
    exclusion criteria over subject features, screening in order to a target
    enrollment, with shortfall reporting.
  - Pure logic, no database access.
- **Persistence (2026-10-26):** migration 10 adds `trial_enrollments` and
  `trial_adverse_events`. `storage::save_trial_protocol` stores criteria on
  `clinical_trial_cohorts`; `screen_and_persist` records enrolled and
  screen-failed decisions in one transaction; `record_adverse_events` validates
  grades before writing; `load_ae_summary` reads them back. The DB-backed test
  is `#[ignore]`d (needs `TEST_DATABASE_URL`); JSON round-trip tests run by default.
- **API (2026-10-26):** `POST /api/v1/trials`, `POST /api/v1/trials/:id/screen`,
  `POST /api/v1/trials/:id/adverse-events`, and
  `GET /api/v1/trials/:id/adverse-events/summary`, each with an explicit `POLICY`
  entry (`clinical_trial` class; write for mutations, read for the summary).
  `extract_cohort_id` now treats `/api/v1/trials/:id` as a cohort scope, so a
  token scoped to one trial cannot reach another, and creating a trial requires
  a `*`-scoped token. Policy tests pass; the handlers themselves have no
  DB-backed test.
- **Still deferred:** a
  richer protocol model (arms, visits), and EHR/FHIR resource coverage beyond the Phase 2
  subset. The FHIR scope has no written requirement yet, so it is not
  hardened speculatively.
- **Plan:** expose the storage functions through `tpt-soma-api` routes with an
  explicit `clinical_trial` policy entry, so the Phase 0 capability and audit
  choke-points cover them.

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
- **Status:** query-builder backend **implemented**; 3D renderer and interactive UI deferred.
  - `crosstalk::run_systemic_query` simulates an OSG topology (node rows and
    `cross_talk` edge rows) and reports the target node's final value, peak
    and peak time. Nothing is persisted.
  - `POST /api/v1/systemic/query` exposes it. It requires a `simulation_output`
    token with at least `read` (an explicit `POLICY` entry in `auth.rs`, so it
    is not an unlisted route), and every call is written to the audit ledger by
    the router-wide middleware. Input errors return 400.
- **Still deferred:** the deck.gl/three.js whole-body renderer. It needs
  anatomical geometry that is not in the repo. The interactive query UI is
  also open.
- **Plan:** source anatomical geometry for the macro nodes first, then build the
  UI on top of `POST /api/v1/systemic/query`.

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
- **Status:** in-process end-to-end test **implemented**; DB-backed run and
  load test still open.
  - `crates/tpt-soma-simulacrum/tests/systemic_query.rs` runs OSG-shaped rows
    through `build_from_topology` and `run_systemic_query`. It checks the
    adipose → IGF-1 → breast chain against an uncoupled control, and checks
    determinism.
  - Still open: running the same query against a populated Plexus OSG
    (needs `TEST_DATABASE_URL` and the Keystone extensions), and the
    full-stack load test (needs target hardware).
- **Plan:** extend `crates/tpt-soma-api/tests/e2e_flight.rs` and the
  `scripts/loadtest` suite once a populated OSG is available.

### 2.10 Code pass notes (2026-10-25)
- Topology loader: `crosstalk::build_from_topology` turns OSG node and
  `cross_talk` edge rows into a runnable system, rejecting unknown kinds,
  duplicate or dangling ids, non-`cross_talk` edges, and undefined couplings.
  The canonical example is now built through this path.
- Not touched in this pass: §2.1 (PyTorch/JAX bridge; the decision above
  still holds until a concrete gradient-fitting use case exists), §2.2
  (Plexus `create_edge` signature check; needs the deployed extension), §2.7
  (partner-site pilot; operational), and the load tests (hardware).
- Verification: `tpt-soma-simulacrum`, `tpt-soma-pathos`, `tpt-soma-clinica`,
  and `tpt-soma-api` tests pass (the API run has 37 unit tests plus 16 in-memory
  middleware tests; DB-backed tests are ignored), and clippy with `-D warnings`
  is clean for these four crates.

### 2.11 Audit-chain alerting (2026-10-26)
- `verify_chain` no longer prints; `ChainReport.failure` carries the first bad
  event's id, position and kind. `alert::check_and_alert` passes failures to
  `AlertSink`s (`TracingAlertSink` in the library; the CLI sink also runs an
  operator-supplied program, executed directly with no shell, with
  `AUDIT_ALERT_*` environment variables).
- Not built: a webhook or pager sink, and a scheduled runner outside
  `audit-cli watch`. The DB-backed tamper tests remain `#[ignore]`d.

## 3. Consequences
- The roadmap's security/compliance requirements are never dropped by
  deferring domain breadth; every deferred path still must enter through the
  Phase 0 capability/audit/DP choke-points.
- Deferred items remain explicitly tracked here rather than silently dropped,
  so future passes have a single source of truth.
