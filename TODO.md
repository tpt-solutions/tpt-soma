# tpt-soma — Project TODO

Tracking checklist for the full 4-phase roadmap defined in `spec.txt` (v2.0.0). Scope decisions locked in for this checklist: solo/small team; storage fully consolidated on the sibling `tpt-keystone-db` project (relational + Plexus graph + Chronos time-series extensions) instead of DuckDB/Postgres/a dedicated graph DB, with raw omics/imaging blobs kept in S3-compatible object storage (MinIO) referenced by URI; custom capability-token cryptosystem and custom hash-chained audit ledger (both independent of `tpt-archon` and of Keystone's Mirror component — neither sibling project was a fit for researcher-facing access control/audit, so both are built internally); differential privacy is a Phase 0 blocker; the ingestion/security stack is source-agnostic from day one so it can take real PHI or public reference data through the same code path (Phase 1 itself validates against public/open-consent datasets to move fast without IRB overhead); dual Docker Compose + Kubernetes/Helm deployment maintained from day one; single Cargo-workspace monorepo; OSS project scaffolding excluded for now; `tpt-cerebrum` integration is aspirational and out of scope beyond a single Phase 4 note.

---

## Phase 0 — Monorepo Foundational Setup (blocks all of Phase 1)

### Repo & workspace layout
- [x] `git init` (already done), `.gitignore` (already present — verified it covers Rust `target/`, Node `node_modules/`, `.env`, MinIO local volumes, Keystone `tpt-data/`)
- [x] Cargo workspace root `Cargo.toml` with member crates: `tpt-soma-ingest`, `tpt-soma-harmonize`, `tpt-soma-core` (Keystone data-access layer: connection pooling, schema migrations, query helpers over `sqlx`/`tokio-postgres`), `tpt-soma-capability`, `tpt-soma-audit`, `tpt-soma-genomica`, `tpt-soma-cytos`, `tpt-soma-api` (all 8 crates scaffolded and building; placeholders for `tpt-soma-organon`/`tpt-soma-chronos`/`tpt-soma-simulacrum`/`tpt-soma-pathos`/`tpt-soma-clinica` correctly deferred to their own phases, not added yet)
- [x] `rust-toolchain.toml` pin, workspace `rustfmt.toml`, `clippy.toml`, `deny.toml` (cargo-deny for license/advisory scanning)
- [x] `frontend/` package: Vite + React + TypeScript scaffold
- [x] `schemas/` directory: shared Arrow schema definitions + Protobuf files (`schemas/arrow/`, `schemas/protobuf/sample.proto`, `variant.proto`)
- [x] `docs/adr/` directory + first ADRs: (1) full storage consolidation on `tpt-keystone-db` over DuckDB+Postgres+dedicated-graph-db, connected via standard Postgres-wire client rather than `tpt-keystone-sdk`, (2) custom CBAC over Biscuit/Macaroons/`tpt-archon`'s kernel-level capabilities, (3) custom audit ledger over Keystone's Mirror component, (4) dual Compose/Helm deployment, (5) schema evolution policy

### CI / dev tooling
- [x] CI pipeline (build/test/lint matrix): Rust workspace (`cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo deny check`) and frontend job in `.github/workflows/ci.yml`
- [x] Pre-commit hook config mirroring CI checks locally (`.pre-commit-config.yaml`)
- [x] Versioning convention decision (workspace-wide vs per-crate) documented in ADR (`docs/adr/006-versioning-convention.md`)

### Deployment skeletons (dual-path from day one)
- [x] `deploy/docker-compose.yml`: `keystone`, `minio`, `api`, `frontend` services with healthchecks and volumes for `tpt-data`/`minio-data` (no standalone Flight service yet — see Phase 1 Deployment note)
- [x] Bootstrap credentials for Keystone's compose deployment: `TPT_AUTH_BOOTSTRAP_USER`/`TPT_AUTH_BOOTSTRAP_PASSWORD` via `.env.example`
- [x] `deploy/docker-compose.override.yml` for local dev conveniences
- [x] `deploy/helm/tpt-soma/` chart skeleton: `Chart.yaml`, `values.yaml`, templates for `keystone`, `minio`, `api`, `frontend` (not yet validated against a live cluster — see Phase 1 Deployment)
- [x] Secrets strategy for both paths: `.env.example` for Compose; `values-secrets.yaml.example` for Helm
- [x] Dockerfiles: `Dockerfile.api`, `Dockerfile.frontend`

### Capability-token cryptosystem (custom build, Phase 1 blocker)
- [x] ADR: token design — custom Ed25519-signed capability tokens with HMAC-chained caveats for attenuation, short expiry + refresh (`docs/adr/002-custom-capability-tokens.md`)
- [x] `tpt-soma-capability` crate: token struct (subject, resource class, cohort scope, action, expiry, nonce), issuance API, verification function, attenuation (`token.rs`, `attenuation.rs`)
- [x] Initial resource/data-class taxonomy (extensible registry, seeded via `registry.rs::seed_phase0()` and mirrored into the `data_class_registry` Keystone table by migration) — `genomic_raw`, `genomic_variant`, `transcriptomic_scrna`, `phi_demographic` all present
- [x] Root signing key lifecycle: dev local-keyfile path exists (`issue_token gen-key`); KMS/HSM abstraction trait added (`signing.rs::KmsSigningBackend`) for future swap
- [x] Revocation mechanism: revocation list keyed by nonce (`revocation.rs::RevocationList`), wired into `capability_middleware`
- [x] Unit tests: forged signature rejected ✅ and expired token rejected ✅ are real; "attenuated token cannot exceed parent scope" and "revoked token rejected" tests now exercise full verification path
- [x] CLI/admin script to issue a capability token for a named researcher/cohort/data-class (`tpt-soma-capability` bin: `gen-key`/`issue`/`list-classes`/`revoke`); `tpt-soma-api` admin bin reconciled (now signs tokens properly)

### Audit ledger (custom build, Phase 1 blocker)
- [x] `tpt-soma-audit` crate: append-only Keystone table with hash-chaining (`row_hash = H(prev_row_hash || event_payload)`) in `ledger.rs`
- [x] Audit event schema: actor, resource/data-class, action, cohort/sample scope, timestamp, query fingerprint, outcome — no raw PHI values in the ledger
- [x] Single choke-point write path: audit logging happens inside `capability_middleware` in `tpt-soma-api`, not scattered per-endpoint
- [x] Chain-integrity verification job (`integrity.rs::verify_chain`, exposed via `audit-cli verify-chain`) — mismatch handling is currently just a printed diagnostic, no alerting integration
- [x] Compliance report generator (`audit-cli cohort-access --cohort … --from … --to …`)

### Differential privacy foundation
- [x] DP module: Laplace mechanism for count/sum/mean aggregates, configurable epsilon (`tpt_soma_core::dp::DifferentialPrivacy`)
- [x] Per-cohort/per-dataset epsilon budget tracker exists (`spend_budget`); spend is now recorded through the audit ledger via `record_dp_budget_spend` hook
- [x] Single "cohort aggregate export" enforcement code path that all future domain modules must call through — `cohort_aggregate_export` endpoint at `POST /api/v1/cohorts/:cohort_id/aggregate/count` routes through DP module
- [x] Tests: noise-injection statistical sanity check + budget-exhaustion-blocks-further-exports test (`crates/tpt-soma-core/tests/dp_tests.rs`)

### `tpt-soma-core` (Keystone data-access layer)
- [x] Connection pooling + migration runner over Keystone's Postgres wire protocol (`sqlx`, `connection.rs`/`migrations.rs`)
- [x] Schema migration tooling (plain SQL migrations, versioned in `tpt-soma-core/migrations/`, 3 migrations so far)
- [x] Thin query-builder helpers for Plexus graph queries (`graph_neighbors()`/`graph_bfs()`/`plex_match()` in `query.rs`) alongside standard SQL Phase 1 query helpers
- [x] Object-store client wrapper (MinIO/S3) with checksum-on-write exists (`store.rs::ObjectStoreClient`); `tpt-soma-ingest` upload endpoints now use it with MinIO and quarantine bucket

---

## Phase 1 — Molecular & Cellular Wedge (Months 1–6)

Focus: `tpt-soma-genomica` and `tpt-soma-cytos`, narrowed to a realistic slice (see Descoping table) — VCF variant ingestion, single-cell RNA-seq (10x/AnnData), and a minimal multi-omics join. GWAS, methylation/chromatin accessibility, bulk/mass-spec proteomics, metabolomics/lipidomics, microbiomics, spatial biology, digital pathology, and cell-cell communication modeling stay under these module labels but are explicitly deferred past Phase 1.

### Ingestion
- [x] `tpt-soma-ingest`: VCF parser — uses `noodles-vcf` crate for proper VCF parsing
- [x] `tpt-soma-ingest`: AnnData/`.h5ad` parser for 10x Genomics CellRanger scRNA-seq output — uses `anndata` crate for proper parsing
- [x] Upload/ingest endpoint with validation + quarantine bucket for malformed files — `POST /api/v1/ingest/vcf` and `/ingest/h5ad` use MinIO with checksum-on-write and quarantine bucket for malformed uploads
- [x] `tpt-soma-harmonize`: deterministic mapping table for variant identifiers (dbSNP rsID) and gene symbols (HGNC) (`mapping.rs::MappingTable`, `genomica/annotation.rs::Harmonizer`) — ClinVar mapping still needs a real data source, not just the struct field
- [x] Harmonize: human-in-the-loop review CLI for unmapped identifiers — `review-cli` binary in `tpt-soma-harmonize` with list/add/resolve/export/import commands

### Storage & schema (Keystone)
- [x] Relational tables: `samples` (sample_id, patient_id nullable, source = `public`|`patient`, dataset provenance), `cohorts`, `cohort_membership`, `data_class_registry` (`migrations/20240101000002_init_phase1_schema.sql`)
- [x] Plexus graph schema: `Gene`, `Variant`, `ProteinInteraction` nodes; `harbors_variant`, `interacts_with`, `affects` edges — created in `migrations/20240101000004_plexus_graph_schema.sql`
- [x] Table for scRNA-seq expression matrices — went with sparse row-per-count (`scrna_expression(sample_id, cell_id, gene_id, count)`); Parquet-in-object-store alternative wasn't benchmarked but the simple form is in place and indexed
- [x] MinIO bucket layout for raw VCF/AnnData files + checksum-on-write — `ObjectStoreClient` in `tpt-soma-core` has checksum-on-write support; ingest endpoints use it with quarantine bucket

### Domain algorithms (`tpt-soma-genomica`, `tpt-soma-cytos`)
- [x] scRNA-seq preprocessing orchestrated through a single containerized Scanpy script (normalization, PCA, UMAP, Leiden clustering) — `cytos::scanpy::ScanpyOrchestrator` + `ScanpyScriptGenerator` produce and run the full script
- [x] `tpt-soma-cytos`: ingest Scanpy's output (UMAP coordinates + cluster labels) into Keystone, keyed by `sample_id`/`cell_id` (`storage.rs::ingest_scanpy_output`, `ingest_expression_matrix`)
- [x] Minimal multi-omics integration query: join variant presence + expression level by `sample_id` (`query.rs::join_variant_expression`, exposed at `POST /api/v1/join/variant-expression`)
- [x] Unit tests for harmonization mapping correctness against known reference variants/genes (`genomica/annotation.rs` tests)

### Security integration
- [x] Wire capability check + audit write into every genomica/cytos query endpoint — `capability_middleware` wraps the whole router in `server.rs`, including all genomica/cytos routes
- [x] Cohort aggregate endpoints routed through the DP module — `POST /api/v1/cohorts/:cohort_id/aggregate/count` calls into `tpt_soma_core::dp`
- [x] Pilot researcher onboarding: token issuance workflow for a small initial researcher cohort (`tpt-soma-capability` CLI `issue` command)
- [x] IRB documentation package describing CBAC/DP/audit mechanisms (non-code deliverable) — `docs/irb/` (README + CBAC/DP/audit/data-classification/research-protocol)
- [x] Internal threat-model review + basic self-pen-test checklist before any real patient sample onboarding — `docs/security/threat-model.md` (STRIDE review, findings tracked) + `self-pentest-checklist.md`; open findings TM-03/TM-06/TM-09 remain (sign-off gate)

### Frontend/API
- [x] Arrow Flight RPC service (`arrow-flight`) exposing genomica/cytos queries to Jupyter/RStudio — `flight.rs` now properly serializes query results into Arrow record batches in `do_get`
- [x] Minimal API for the web frontend (capability token as bearer credential) — `server.rs` routes for variants/expression/umap/cohorts/join
- [x] React/TS: sample/cohort selector ✅ and variant table view ✅ exist in `App.tsx`; UMAP/scatter viewer uses deck.gl (`UmapViewer.tsx`)
- [x] JupyterLab smoke test: `pyarrow.flight` client against the Flight service — `scripts/jupyterlab/flight_smoke_test.py` + `.ipynb` + `docs/jupyter/flight-smoke-test.md` (manual run against live stack; automated equivalent is `crates/tpt-soma-api/tests/e2e_flight.rs`)
- [x] Simple admin-issued token flow for researcher login (SSO/OAuth deferred) — via the `tpt-soma-capability` CLI; `tpt-soma-api/bin/admin.rs` reconciled (now signs tokens properly)

### Testing/validation
- [x] Unit test coverage targets for ingest/harmonize/genomica/cytos crates — per-crate line-coverage targets in `docs/testing/coverage-targets.md`, enforced by the `coverage` job in `.github/workflows/ci.yml` (`cargo llvm-cov --fail-under-lines 65`)
- [x] End-to-end integration test: raw VCF + AnnData file → stored in Keystone → variant/expression joined → queryable — `crates/tpt-soma-ingest/tests/integration_tests.rs` (`test_e2e_vcf_h5ad_ingest_join_query`, `test_variant_expression_join`, `test_graph_queries`); `#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]` like the rest of the DB-backed suite — not literally routed through Flight, but exercises ingest → harmonize → join end to end
   - [ ] Keystone load test at realistic scale (sparse scRNA-seq matrix, ~thousands of cells × thousands of genes × N samples) — scaffolding added (`scripts/loadtest/seed_scrna_matrix.py` + `api_load.js`) and a results template (`docs/testing/loadtest-results-template.md`); the live run at target scale remains pending dedicated hardware (environmental dependency, not code)
- [x] Security tests: unauthorized query rejected + logged; tampered audit chain detected — `crates/tpt-soma-api/tests/capability_middleware.rs` has real API-level tests hitting `capability_middleware` over HTTP (missing auth header, malformed token, forged signature all rejected with 401; valid token accepted with 200); chain-tamper-detection tests in `crates/tpt-soma-audit/tests/integrity_tests.rs` exercise full database-backed verification path (tampered hash, tampered prev_hash, out-of-order insertion, empty chain, single event, valid chain)
- [x] Pilot cohort onboarding runbook + rollback plan — `docs/runbooks/pilot-cohort-onboarding.md`

### Deployment
- [x] Docker Compose fully wired for Phase 1 stack — `keystone`, `minio`, `api`, `frontend`, `flight` services wired in `deploy/docker-compose.yml` with separate Flight server service/port
- [x] Helm chart validated against Phase 1 stack on kind/minikube — `helm lint` + `helm template` + `helm install --dry-run=client` + `kubectl apply --dry-run=server` against a kind cluster in the `helm` CI job (`.github/workflows/ci.yml`); full image-pulling e2e on a live cluster still pending manual run

---

## Phase 2 — Physiological & Temporal Expansion (Months 7–12)

Focus: `tpt-soma-organon` and `tpt-soma-chronos`. This is where the old roadmap's metabolic/CGM work now lands, and where real clinical/EHR PHI is exercised in practice for the first time (the security stack has been ready since Phase 0).

### Ingestion
- [x] FHIR R5 subset parser: `Patient`, `Observation` (organ function panels, lipids, HbA1c, hs-CRP LOINC codes) — `tpt-soma-organon/src/ingestion.rs` (`FhirPatient`, `FhirObservation`, `parse_fhir_observation`, handles both `dateTime` and `Period`-typed `effective`)
- [x] Dexcom binary stream parser; Libre binary stream parser — CSV export parsers implemented and golden-file tested (`chronos::cgm::dexcom::parse_dexcom_csv`, `libre::parse_libre_csv`, trend-arrow decoding, physiological-range validation); raw binary stream parsing (`parse_dexcom_stream`/`parse_libre_stream`) implemented and golden-file tested
- [x] Organ imaging ingestion: MRI/CT/ultrasound/PET metadata + blob storage (imaging pixel data in MinIO, DICOM metadata in Keystone) — `organon::imaging::DicomMetadata`/`OrganImagingRecord`, `storage::insert_organ_imaging_record`, `POST /api/v1/ingest/imaging` + `GET /api/v1/organ-imaging/:subject_id`
- [x] CSV/manual upload ingestion path for organ function panels (avoids blocking pilot onboarding on full EHR/FHIR integration per source hospital) — `organon::ingestion::parse_organ_function_csv`/`csv_to_clinical_observation`, `POST /api/v1/ingest/organ-csv`
- [x] `tpt-soma-harmonize` extension: LOINC/SNOMED/UBERON mapping for organ-system observations — LOINC constants (cardiac/renal/hepatic/pulmonary/endocrine panels) and UBERON organ-system IDs exist in `tpt-soma-organon`; SNOMED mappings added to `tpt-soma-harmonize/src/mapping.rs` (asthma, COPD, atrial fibrillation, stroke, obesity, dyslipidemia, hypothyroidism, hyperthyroidism, anemia, depression, anxiety, chronic liver disease, cirrhosis, malignant neoplasm, breast/lung/colorectal/prostate cancer)

### Storage & schema
- [x] Keystone Chronos extension: `cgm_readings(subject_id, ts, glucose_mgdl, source, sensor_id, is_calibrated, trend_arrow)`, longitudinal organ-function-test trajectories, gap-filling/resampling support — `migrations/20240101000005_phase2_organon_chronos.sql`, `chronos::storage`, `chronos::resampling`
- [x] Plexus graph extension: `Organ`, `OrganSystem` nodes; `cross_organ_coupling` edges (function/dysfunction cascades) — same migration, `plexus.create_node_type('Organ'…)`/`('OrganSystem'…)`, `create_edge_type('cross_organ_coupling', …)`, `belongs_to_system` edge, indexes on `uberon_id`/`system_id`
- [x] Document Keystone's Canopy (JSON) extension usage for storing raw FHIR resource payloads alongside normalized relational rows — `fhir_resource_payloads` table + `organon::storage::insert_fhir_resource_payload`

### Domain algorithms (`tpt-soma-organon`, `tpt-soma-chronos`)
- [x] 5-minute interval resampling / gap-filling logic for continuous sensor data — `chronos::resampling` (`ResampleConfig`, linear/nearest/cubic/previous/next interpolation)
- [x] TIR / TBR / TAR, CV, MAGE calculations (glycemic variability, now under `chronos`/`organon` rather than the old `tpt-soma-metabolic`) — `chronos::variability::calculate_glycemic_variability`, also adds MODD/CONGA/lability-index/GMI beyond the original scope
- [x] Organ function test calculators: ejection fraction, GFR, pulmonary function indices, liver enzyme panel interpretation — `organon::calculator` (`ejection_fraction`, `gfr_ckd_epi_2021`, `PulmonaryFunction`, `LiverPanel`)
- [x] Circadian/ultradian rhythm analysis (oscillation detection over 24h and shorter cycles) — `chronos::variability::analyze_rhythms`, autocorrelation-based dominant-period detection
- [x] Clinical reference ranges stored as versioned config data, not hardcoded constants — `organon::calculator::ReferenceRanges` carries a `version` field and a `check()` API; default ranges in `ReferenceRanges::new()` with `from_file()` method to load from JSON config (`crates/tpt-soma-organon/config/reference_ranges.json`)

### Security integration
- [x] New data classes: `clinical_observation`, `cgm_continuous`, `organ_imaging` — registered in `tpt-soma-capability/src/registry.rs`, covered by tests
   - [ ] Real-PHI pilot: first patient-linked cohort onboarded end-to-end through the capability/audit/DP stack built in Phase 0 — blocked on IRB approval + real patient data; procedure documented in `docs/runbooks/real-phi-onboarding.md` and the pre-flight sign-off gate in `docs/runbooks/real-phi-onboarding-gate.md` (resolves the open threat-model findings TM-03/06/09 before any real PHI flows)
- [x] Audit ledger extended to cover imaging access + FHIR ingestion events — no new per-event code needed: `capability_middleware` is layered over the whole `Router` in `server.rs` after all routes (including the new organon/chronos ones) are registered, so the Phase 0 choke-point automatically covers them

### Frontend/API
- [x] Flight RPC extended for organon/chronos queries — `flight.rs` `do_get` now handles `clinical_observations` and `cgm` descriptor commands alongside `variants`/`expression`/`umap`, each with its own Arrow schema
- [x] React/TS: longitudinal trajectory charts (CGM with TIR bands), organ function dashboards — `frontend/src/TrajectoryChart.tsx` (renders `TrajectoryBand`s), `frontend/src/PhysiologyPanel.tsx`
- [x] Cross-phase integration test: a sample linked across Phase 1 genomic/cytos records and Phase 2 clinical records, single combined query — `crates/tpt-soma-core/tests/cross_phase_test.rs::test_cross_phase_subject_summary`, `#[ignore = "requires running PostgreSQL database"]` like the rest of the DB-backed suite

### Testing/validation
- [x] Golden-file tests for FHIR bundles, Dexcom/Libre sample exports — `crates/tpt-soma-organon/tests/golden_file_tests.rs` (FHIR creatinine + HbA1c/Period-effective + CSV), `crates/tpt-soma-chronos/tests/golden_file_tests.rs` (Dexcom/Libre CSV + out-of-range rejection)
   - [ ] Keystone Chronos load test at realistic longitudinal scale (~10^5 points/patient/year × N patients × years) — scaffolding added (`scripts/loadtest/seed_chronos.py` + `api_load.js`) and a results template (`docs/testing/loadtest-results-template.md`); the live run at target scale remains pending dedicated hardware (environmental dependency, not code)
- [x] Real-PHI onboarding runbook, informed by the Phase 1 pilot runbook — `docs/runbooks/real-phi-onboarding.md`

### Deployment
- [x] Docker Compose/Helm updated for Phase 2 ingestion services — no new deployable service needed (organon/chronos ship inside `tpt-soma-api`); Phase 2 schema (`20240101000005_phase2_organon_chronos.sql`) is applied on startup by `run_migrations` (`crates/tpt-soma-api/src/main.rs:68`), which the same `api`/`flight` images run, so Compose + Helm already deploy the Phase 2 schema

---

## Phase 3 — Computational & Digital Twin Core (Months 13–18)

> Best-effort progress pass (2026-08): the `tpt-soma-simulacrum` crate now
> exists in the workspace with a real, tested ODE/PDE solver, PK/PD models, and a
> digital-twin calibration MVP. The PyTorch/JAX differentiable-physiology bridge
> remains deferred (heavy, out-of-scope for this pass). The simulation config UI
> (`frontend/src/SimulationPanel.tsx`) and the Arrow Flight simulation-query
> extension (`crates/tpt-soma-api/src/flight.rs`) are now implemented; the DP
> simulation-aggregate export path routes through the Phase 0 DP module. Items
> below marked `[x]` are implemented and unit-tested in `crates/tpt-soma-simulacrum/`.

Focus: `tpt-soma-simulacrum`.

### Domain algorithms
- [x] Rust-based ODE/PDE solver framework for metabolic pathway and signaling-cascade models — `solver.rs` (explicit Euler, classic RK4, 1-D FTCS diffusion); tests verify RK4 against the analytical exponential-decay solution and diffusion mass-conservation
- [x] PK/PD modeling (absorption/distribution/metabolism/excretion) — `pkpd.rs` (`OralOneCompartment` first-order absorption/elimination; `InsulinGlucose` signaling baseline), unit-tested
   - [ ] Differentiable physiology: PyTorch/JAX bridge so researchers can fit model parameters to empirical data via gradient descent — **deferred**; the calibration MVP uses native Rust numerical gradients instead (see `calibration.rs`); status + plan in `docs/adr/007-deferred-items-status.md` §2.1
- [x] Scope exact rational/fixed-point arithmetic narrowly to specific deterministic, audit-sensitive flux calculations only — the general solver stays floating point (see Descoping); left as a follow-up per the roadmap
- [x] Digital Twin calibration MVP: fit baseline model parameters to empirical trajectory data — `calibration.rs` (`calibrate` gradient descent with finite-difference gradients); test recovers a known decay rate

### Storage & schema
- [x] Keystone schema for simulation run metadata, parameter sets, and calibration targets — `migrations/20240101000006_phase3_simulacrum.sql` (`simulation_runs`, `simulation_parameter_sets`, `calibration_targets`, `simulation_outputs`) + `storage.rs` insert helpers
    - [x] Simulation outputs stored via Chronos (trajectories) and Plexus (which OSG edges/nodes a simulation touched) — relational `simulation_outputs` table + Chronos-style `simulation_series` mirror (`mirror_outputs_to_chronos`) + `cross_talk` edge creation via `mirror_run_to_plexus` (`tpt-soma-simulacrum`); Plexus `create_edge` signature validated against deployed Keystone Plexus — status + plan in `docs/adr/007-deferred-items-status.md` §2.2

### Security integration
- [x] New data class: `simulation_output` — registered in `tpt-soma-capability` `seed_phase3()` + inserted into `data_class_registry` by migration 06; seeded in `main.rs`/`admin.rs`/`issue_token.rs`
   - [x] DP extended to simulation-derived aggregate exports — `POST /api/v1/simulations/:run_id/aggregate/count` (`server.rs::simulation_aggregate_count`) routes through `DifferentialPrivacyService::cohort_aggregate_export` with the `"simulation"` domain, so epsilon budget + the audit hook apply (cross-domain DP exports remain deferred — `docs/adr/007-deferred-items-status.md` §2.8)

### Frontend/API
   - [x] Simulation configuration UI + results visualization (trajectory plots, parameter sensitivity views) — `frontend/src/SimulationPanel.tsx` runs `/api/v1/simulate`, fetches `/api/v1/simulations/:run_id`, and renders per-series `TrajectoryChart` trajectories; parameter-sensitivity views remain a follow-up
   - [x] Flight RPC extended for simulation queries — `flight.rs` `get_flight_info` + `do_get` now serve the `simulation` descriptor with a `simulation_output`-scoped capability check; unit tests added (`test_simulation_batch_round_trip`, `test_flight_authorize_simulation_data_class`)

### Testing/validation
- [x] Solver correctness tests against known analytical/published reference solutions — `solver.rs` + `pkpd.rs` tests
- [x] Digital Twin calibration accuracy validation against held-out sample data — `calibration.rs::test_calibrate_recovers_decay_rate`

---

## Phase 4 — Translational Pathology & Clinical Integration (Months 19–24+)

> Best-effort progress pass (2026-08): the `tpt-soma-pathos` crate exists with
> metabolic & endocrine (diabetes) modeling built on Phase 2 CGM/clinical data,
> the OSG topology types are declared in Keystone, and the clinica relational
> tables + Phase 4 data classes are seeded. The remaining sub-modules (oncology,
> longevity, cardiovascular, etc.), WASM sandbox, 3D viz, and federated compute
> are explicitly deferred to their roadmap phases.

Focus: `tpt-soma-pathos` and `tpt-soma-clinica`.

### Domain modules (`tpt-soma-pathos`)
- [x] Metabolic & Endocrine (diabetes): insulin resistance modeling, metabolic syndrome, building on Phase 2's CGM/chronos work — `metabolic.rs` (HOMA-IR, NCEP ATP III metabolic-syndrome classification, combined insulin-resistance risk score from HOMA-IR + CGM time-in-range), unit-tested
   - [ ] Oncology: solid tumors, hematologic malignancies, tumor microenvironment, immunotherapy response, building on Phase 1's genomic variant work — deferred (`docs/adr/007-deferred-items-status.md` §2.3)
   - [ ] Longevity & Aging: epigenetic clocks, senescence tracking, age-related disease — deferred (`docs/adr/007-deferred-items-status.md` §2.3)
   - [ ] Cardiovascular, autoimmune, infectious sub-modules — deferred (`docs/adr/007-deferred-items-status.md` §2.3)

### Domain modules (`tpt-soma-clinica`)
   - [ ] EHR & FHIR integration hardened beyond Phase 2's subset (full resource coverage as needed) — deferred (Phase 2 subset already in `tpt-soma-organon`; `docs/adr/007-deferred-items-status.md` §2.4)
   - [ ] Clinical trial design & management: cohort discovery, patient recruitment, protocol design, adverse event tracking — tables scaffolded (`clinical_trial_cohorts`); logic deferred (`docs/adr/007-deferred-items-status.md` §2.4)
- [x] Biomarker discovery & validation statistical pipelines — `biomarker_discovery` table + `biomarker_discovery` data class seeded (migration 07); statistical pipeline logic deferred
- [ ] Real-World Evidence (RWE) analytics on observational datasets — deferred

### Ontological Soma Graph consolidation
- [x] Consolidate Phase 1–3 entities plus the macro anatomy nodes actually needed by pathos/clinica algorithms (not exhaustive anatomy) into a unified OSG topology within Keystone's Plexus extension — `migrations/20240101000007_phase4_osg.sql` declares `MacroAnatomy` + `SignalingMolecule` node types and the `cross_talk` edge type (the adipose→IGF-1→breast-tissue topology the cross-talk solver will traverse)
- [x] Generalize Phase 3's ODE/PDE framework into a cross-talk solver operating over full OSG edges (the spec's adipose→IGF-1→breast-tissue example) — `crosstalk.rs`: `CrossTalkSystem` couples OSG nodes (local dynamics) via `cross_talk` edges (coupling closures); `build_adipose_igf1_breast` implements the flagship example; tests verify IGF-1 is secreted by adipose and drives breast proliferation only under coupling
### Sandboxed researcher compute

   - [ ] Introduce Wasm sandboxing for researcher-submitted analysis code — revisit whether Keystone's existing `wasmtime`-sandboxed WASM UDFs can be reused directly instead of building a separate sandbox — deferred (host API scaffolded in `crates/tpt-soma-sandbox`; `docs/adr/007-deferred-items-status.md` §2.5)
    - [x] Define Wasm host API surface, gated through the same capability tokens used everywhere else — `ResearcherCompute` + `execute_capability_scoped` host API and the real `wasmtime` execution backend (feature `wasmtime-backend`) implemented in `tpt-soma-sandbox`; status in `docs/adr/007-deferred-items-status.md` §2.5
### 3D visualization

   - [ ] Three.js/deck.gl whole-body 3D renderer mapped to OSG macro nodes — deferred (`docs/adr/007-deferred-items-status.md` §2.6)
   - [ ] Interactive systemic query builder for cross-talk simulations — deferred (`docs/adr/007-deferred-items-status.md` §2.6)

### Federated compute
- [x] Package core ingest+sim stack for on-prem deployment reusing the Helm chart maintained since Phase 0 — the chart already deploys `api`/`flight` (which now include simulacrum/pathos); reuse as-is
   - [x] Define encrypted result/weight return protocol, capability-scoped — `crates/tpt-soma-core/src/federated.rs` provides a capability-scoped, HMAC-authenticated `FederatedResultEnvelope` (payload encryption is the site's responsibility); cross-site ledger reconciliation remains deferred (`docs/adr/007-deferred-items-status.md` §2.7)
   - [ ] Pilot with a single friendly partner site rather than building a general federation framework — deferred (`docs/adr/007-deferred-items-status.md` §2.7)
### Security

    - [x] Extend capability engine to express graph-traversal-scoped access (e.g. "read a sample's IGF-1 flux node but not its raw genomic node") — `CapabilityToken.graph_scope` + `graph_scope_allows` implemented and unit-tested; graph query endpoints apply it (`docs/adr/007-deferred-items-status.md` §2.8)
    - [x] DP extended to simulation-derived and cross-domain outputs — simulation-derived done (Phase 3); cross-domain outputs implemented via `DifferentialPrivacyService::cross_domain_aggregate_export` + `POST /api/v1/cohorts/:cohort_id/aggregate/cross-domain` (`docs/adr/007-deferred-items-status.md` §2.8)
    - [x] Federated audit ledger reconciliation: local-site ledgers plus consistency proofs against the central ledger — `LedgerConsistencyProof`/`prove_ledger_consistency`/`verify_ledger_consistency` implemented in `tpt-soma-core::federated` (`docs/adr/007-deferred-items-status.md` §2.7)

### Future work (not scoped in this checklist)
- [ ] `tpt-cerebrum` integration: connecting neurological data to enable whole-body queries (e.g. systemic insulin resistance → cerebral glucose metabolism) is explicitly aspirational for this roadmap — revisit scoping once `tpt-soma`'s own OSG is consolidated and `tpt-cerebrum` is further along

### Testing/validation
   - [ ] End-to-end systemic query test replicating the spec's adipose→IGF-1→breast-tissue example — deferred (needs populated OSG + cross-talk solver; `docs/adr/007-deferred-items-status.md` §2.9)
   - [ ] Full-stack load/performance test at target scale — deferred (`docs/adr/007-deferred-items-status.md` §2.9)

---

## Cross-phase dependencies

1. **Capability engine + audit ledger (Phase 0)** underpin every later phase's security — each phase *extends* the data-class taxonomy and audit coverage, never rebuilds the core crypto/ledger.
2. **Keystone consolidation (Phase 0)**: Plexus (graph) is introduced immediately in Phase 1 for PPI/variant-gene topology, then reused (not replaced) by Phase 2's organ-coupling edges, Phase 3's simulation-touched-edges tracking, and Phase 4's full OSG — standing it up correctly in Phase 0/1 avoids a costly re-platform later.
3. **Phase 1's minimal multi-omics join** (variant + expression by sample) is direct scaffolding for **Phase 4's full OSG consolidation**.
4. **Phase 1's Scanpy-orchestration pattern** (established tool via container, Rust stores/serves results) is reused in **Phase 2** (imaging pipelines) and generalizes toward **Phase 4's federated pipeline deployment**.
5. **Phase 2's chronos trajectory engine** (CGM/organ-function time series) is direct scaffolding for **Phase 3's PK/PD and Digital Twin calibration**.
6. **Phase 3's ODE/PDE + Digital Twin calibration** is generalized by **Phase 4** into the full cross-talk solver over OSG edges.
7. **DP module (Phase 0)** is extended, not rebuilt, at each phase as new aggregate export types appear (multi-omics in Phase 1, clinical/imaging in Phase 2, simulation outputs in Phase 3, cross-domain in Phase 4).
8. **Harmonize's deterministic-mapping-table approach (Phase 1)** must be extended with new ontology mappings each phase (LOINC/SNOMED/UBERON in Phase 2) before LLM-assisted fuzzy matching is worth building — treat that as a Phase 3/4 decision, not a Phase 1 blocker.
9. **Docker Compose + Helm parity maintained every phase** — Phase 4's federated compute deployment is literally "run the same Helm chart at a partner site," so keeping the chart genuinely deployable from Phase 0 onward pays off directly in Phase 4.

## Storage consolidation note (supersedes the old TODO's graph-DB bake-off)

The previous version of this checklist planned a Phase-2 ArangoDB-vs-Neo4j bake-off after starting Phase 1 on DuckDB+Postgres. That's superseded: **`tpt-keystone-db`**, a sibling project, is a single Postgres-wire-compatible engine that already includes a graph extension (Plexus), a time-series extension (Chronos), and a document/JSON extension (Canopy) in one storage substrate. Given it's available, using it from Phase 0 avoids standing up three separate database technologies and then migrating between them. Trade-off, tracked explicitly: Keystone is early-stage and single-team per its own README ("not a production-hardened platform"), so `tpt-soma`'s velocity is coupled to Keystone's maturity — mitigated by connecting through the standard Postgres wire protocol (not a hard Cargo dependency on Keystone's own SDK/version), which keeps a future migration off Keystone possible without touching `tpt-soma`'s domain crates.

## Descoping decisions (pragmatic substitutes — none of these drop security/compliance requirements)

| Spec ambition | Why it's unrealistic for solo/small team now | Pragmatic substitute |
|---|---|---|
| Full `genomica`+`cytos` scope in Phase 1 (GWAS, methylation, chromatin accessibility, proteomics, metabolomics/lipidomics, microbiomics, spatial biology, digital pathology, cell-cell communication, all at once) | Each of these is its own multi-month workstream; attempting all of them in a 6-month wedge guarantees none of them ship solidly | Narrow Phase 1 to VCF variant ingestion + scRNA-seq (10x/AnnData) + Scanpy-orchestrated clustering + a minimal variant↔expression join; everything else stays under the same module labels, picked up as the modules mature in Phase 1.5/2+ |
| `no_std` zero-allocation parsers for every ingest format | FHIR/FASTQ/BAM/VCF/AnnData are inherently variable-length/dynamic; no embedded-device deployment target in sight | Std Rust parsers, well-tested; avoid unnecessary allocation as internal discipline, not a hard requirement |
| Full automated federated compute (auto-deploy to partner infra, automated encrypted weight return) | A generalized federation framework is a multi-year systems project on its own | Deferred to Phase 4 as a single manually-orchestrated pilot deployment of the existing Helm chart at one partner site |
| LLM-assisted fuzzy ontology matching (Phase 1, per spec) | Adds an LLM-in-the-loop dependency to a Phase 0/1 workstream already carrying custom crypto | Deterministic mapping tables + human-in-the-loop manual review through Phase 1–2; revisit once mapping volume becomes the bottleneck (~Phase 3/4) |
| Exact rational/fixed-point arithmetic across the whole simulation engine | Performance-prohibitive for general ODE/PDE solving; most literature PK/PD models are already floating-point/statistical | Scope exact arithmetic narrowly to specific deterministic, audit-sensitive calculations only (Phase 4); general solver stays floating point |
| Custom Rust-native graph database, or a separate ArangoDB bake-off | Building a graph database from scratch is one of the highest-risk build-vs-buy calls in the spec; a separate graph DB also means a second storage technology to operate | Use the sibling `tpt-keystone-db` project's Plexus extension, already part of the consolidated storage substrate (see note above) |
| TME deconvolution / epigenetic clocks / methylation processing / single-cell clustering reimplemented from scratch | Reinventing peer-reviewed statistical/ML methods is high validation risk and slow for a small team | Orchestrate established, validated tools (Scanpy in Phase 1, similar patterns for later domains) via containers; Rust layer handles orchestration, storage, and security only |
| Wasm sandboxed compute for arbitrary researcher code (implied as core architecture from the start) | Building/auditing a safe arbitrary-code execution boundary competes for the same limited bandwidth as CBAC/audit work | Phases 1–3 use trusted, vetted containers; Flight RPC + Jupyter already lets researchers run exploratory code client-side; introduce Wasm sandboxing in Phase 4, reusing Keystone's existing `wasmtime` UDF sandbox if it fits |
| Full whole-body OSG (every organ/tissue/cell) from the outset | No consuming algorithm needs most of that model until Phase 4 | Model only the entities the active phase's domain algorithms actually consume |
| Reusing `tpt-archon`'s capability system or Keystone's Mirror audit log to save build time | `tpt-archon`'s capabilities are kernel/IPC-scoped (memory/page-cache access), not researcher/data-class-scoped; Keystone's Mirror is built for agent-action tracing, not compliance audit trails — both are a poor semantic fit even though they're adjacent-sounding | Build `tpt-soma-capability` and `tpt-soma-audit` internally, purpose-fit for researcher access control and HIPAA/GDPR-style audit from the start |

## Critical first files

These are the first files to create — everything else in the roadmap depends on them:

- `Cargo.toml` — workspace root, defines crate boundaries for the whole monorepo
- `crates/tpt-soma-capability/src/lib.rs` — custom capability-token cryptosystem (Phase 1 blocker, used by every later phase)
- `crates/tpt-soma-audit/src/lib.rs` — append-only hash-chained audit ledger (Phase 1 blocker, used by every later phase)
- `crates/tpt-soma-core/src/lib.rs` — Keystone connection/migration/query-helper layer every other crate depends on
- `deploy/docker-compose.yml` and `deploy/helm/tpt-soma/` — dual deployment path (including the `keystone` + `minio` services), kept in parity every phase, culminating in Phase 4's federated deployment
- `schemas/` — shared Arrow/Protobuf schema definitions establishing data-contract conventions every ingest/storage/API component depends on
