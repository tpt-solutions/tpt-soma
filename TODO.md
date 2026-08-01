# tpt-soma — Project TODO

Tracking checklist for the full 4-phase roadmap defined in `spec.txt`. Scope decisions locked in for this checklist: solo/small team, real PHI in scope from day one (CBAC/audit/DP are blocking, not deferred), custom capability-token cryptosystem, DuckDB+Postgres+S3 for Phase 1 storage (dedicated graph db introduced in Phase 2), dual Docker Compose + Kubernetes/Helm deployment maintained from day one, single Cargo-workspace monorepo, OSS scaffolding excluded for now.

---

## Phase 0 — Monorepo Foundational Setup (blocks all of Phase 1)

### Repo & workspace layout
- [ ] `git init`, `.gitignore` (Rust `target/`, Node `node_modules/`, `.env`, DuckDB/MinIO local volumes)
- [ ] Cargo workspace root `Cargo.toml` with member crates: `tpt-soma-ingest`, `tpt-soma-harmonize`, `tpt-soma-core`, `tpt-soma-capability`, `tpt-soma-audit`, `tpt-soma-metabolic`, `tpt-soma-api` (placeholders for `tpt-soma-onco`, `tpt-soma-aevum`, `tpt-soma-systemic`, `tpt-soma-sim` added in later phases)
- [ ] `rust-toolchain.toml` pin, workspace `rustfmt.toml`, `clippy.toml`, `deny.toml` (cargo-deny for license/advisory scanning)
- [ ] `frontend/` package: Vite + React + TypeScript scaffold
- [ ] `schemas/` directory: shared Arrow schema definitions + Protobuf/FlatBuffers files, with a documented versioning/compatibility policy (additive-only within major version)
- [ ] `docs/adr/` directory + first ADRs: (1) DuckDB+Postgres over graph-db-now, (2) custom CBAC over Biscuit/Macaroons, (3) dual Compose/Helm deployment, (4) schema evolution policy

### CI / dev tooling
- [ ] CI pipeline (build/test/lint matrix): Rust workspace (`cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`) and frontend (`tsc`, `eslint`, `vitest`/`jest`)
- [ ] Pre-commit hook config mirroring CI checks locally
- [ ] Versioning convention decision (workspace-wide vs per-crate) documented in ADR

### Deployment skeletons (dual-path from day one)
- [ ] `deploy/docker-compose.yml`: postgres, minio (S3-compatible), api service placeholder, frontend dev service, volumes for DuckDB file(s)
- [ ] `deploy/docker-compose.override.yml` for local dev conveniences (hot reload, exposed debug ports)
- [ ] `deploy/helm/tpt-soma/` chart skeleton: `Chart.yaml`, `values.yaml`, templates mirroring compose services 1:1 — must be kept genuinely deployable every phase, not bolted on later
- [ ] Secrets strategy for both paths: `.env.example` for Compose; K8s `Secret`/`values-secrets.yaml.example` for Helm (sealed-secrets/SOPS noted as future hardening)
- [ ] Dockerfiles: multi-stage Rust build, frontend static build, shared image family used by both deploy paths

### Capability-token cryptosystem (custom build, Phase 1 blocker)
- [ ] ADR: token design — custom Ed25519-signed capability tokens with HMAC-chained caveats for attenuation, short expiry + refresh
- [ ] `tpt-soma-capability` crate: token struct (subject, resource class, cohort scope, action, expiry, nonce), issuance API, verification function, attenuation (derive narrower child token without re-signing by root key)
- [ ] Initial resource/data-class taxonomy (extensible registry) — Phase 1 needs `clinical_labs`, `cgm_continuous`, `phi_demographic`
- [ ] Root signing key lifecycle: dev = local keyfile; abstracted behind a trait so prod can later swap in KMS/HSM
- [ ] Revocation mechanism (revocation list keyed by nonce, or short TTL + no revocation for v1 — pick one, document trade-off)
- [ ] Unit tests: forged signature rejected, expired token rejected, attenuated token cannot exceed parent scope, revoked token rejected
- [ ] CLI/admin script to issue a capability token for a named researcher/cohort/data-class

### Audit ledger (custom build, Phase 1 blocker)
- [ ] `tpt-soma-audit` crate: append-only Postgres table with hash-chaining (`row_hash = H(prev_row_hash || event_payload)`)
- [ ] Audit event schema: actor, resource/data-class, action, cohort/patient scope, timestamp, query fingerprint, outcome — no raw PHI values in the ledger
- [ ] Single choke-point write path: audit logging happens inside capability-verification middleware, not scattered per-endpoint
- [ ] Chain-integrity verification job (recompute and compare hash chain; alert on mismatch)
- [ ] Compliance report generator (CLI: "show all access to cohort X in date range")

### Differential privacy foundation
- [ ] DP module: Laplace mechanism for count/sum/mean aggregates, configurable epsilon
- [ ] Per-cohort/per-dataset epsilon budget tracker, spend recorded through the audit ledger
- [ ] Single "cohort aggregate export" enforcement code path that all future domain modules must call through
- [ ] Tests: noise-injection statistical sanity checks; budget exhaustion blocks further exports

---

## Phase 1 — Metabolic Wedge (Months 1–6)

### Ingestion
- [ ] `tpt-soma-ingest`: Dexcom binary stream parser
- [ ] `tpt-soma-ingest`: Libre binary stream parser
- [ ] FHIR R5 subset parser: `Patient`, `Observation` (HbA1c, lipids, hs-CRP LOINC codes) only
- [ ] CSV/manual upload ingestion path for blood panels (avoids blocking pilot onboarding on full EHR/FHIR integration per source hospital)
- [ ] Upload/ingest endpoint with validation + quarantine bucket for malformed files
- [ ] `tpt-soma-harmonize`: deterministic mapping table (local code → LOINC/SNOMED) seeded for CGM units, HbA1c, lipid panel, hs-CRP
- [ ] Harmonize: human-in-the-loop review CLI/UI for unmapped codes (LLM-assist deferred, see Descoping)
- [ ] Golden-file tests using real-format Dexcom/Libre sample exports and sample FHIR bundles

### Storage & schema
- [ ] Postgres migrations: `patients`, `cohorts`, `cohort_membership`, `biomarker_catalog`, `data_class_registry`
- [ ] DuckDB schema: `cgm_readings(patient_id, ts, glucose_mgdl, source)`, `lab_panel_results(...)`; decide Parquet partitioning (per-patient or per-cohort)
- [ ] MinIO/S3 bucket layout + checksum-on-write for raw uploaded files
- [ ] Document Postgres "graph-ish" modeling of patient↔cohort↔consent↔data-class via recursive CTEs (stands in for a graph db in Phase 1)
- [ ] Dev backup/restore scripts for Postgres + DuckDB file + MinIO bucket

### Domain metrics/algorithms (`tpt-soma-metabolic`)
- [ ] 5-minute interval resampling / gap-filling logic
- [ ] TIR / TBR / TAR calculation with configurable clinical thresholds
- [ ] CV (coefficient of variation) calculation
- [ ] MAGE algorithm (peak/nadir detection with SD excursion threshold) + unit tests against published reference values
- [ ] HOMA-IR and Matsuda index calculators with documented input requirements (fasting glucose/insulin, OGTT points)
- [ ] Intervention simulator MVP: literature-derived population PK/PD parameter sets for GLP-1/GIP agonists and SGLT2 inhibitors (seeds Phase 4's cross-talk solver)
- [ ] Clinical reference ranges (ADA/ISPAD) stored as versioned config data, not hardcoded constants

### Security integration
- [ ] Wire capability check + audit write into every metabolic query endpoint
- [ ] Cohort aggregate endpoints routed through the DP module
- [ ] Pilot researcher onboarding: token issuance workflow for the small diabetes-researcher cohort
- [ ] IRB documentation package describing CBAC/DP/audit mechanisms (non-code deliverable, required before real PHI flows)
- [ ] Internal threat-model review + basic self-pen-test checklist before PHI onboarding

### Frontend/API
- [ ] Arrow Flight RPC service (`arrow-flight`) exposing metabolic queries to Jupyter/RStudio
- [ ] Minimal API for the web frontend (capability token as bearer credential)
- [ ] React/TS: cohort/patient selector, CGM trajectory chart with TIR bands, glycemic metrics dashboard
- [ ] JupyterLab smoke test: `pyarrow.flight` client against the Flight service
- [ ] Simple admin-issued token flow for researcher login (SSO/OAuth deferred)

### Testing/validation
- [ ] Unit test coverage targets for ingest/harmonize/metabolic crates
- [ ] End-to-end integration test: raw CGM file → stored → metrics computed → queryable via Flight
- [ ] DuckDB load test at realistic scale (~105k points/patient/year × N patients × years)
- [ ] Security tests: unauthorized query rejected + logged; tampered audit chain detected
- [ ] Pilot cohort onboarding runbook + rollback plan

### Deployment
- [ ] Docker Compose fully wired for Phase 1 stack (postgres, minio, api, flight server, frontend)
- [ ] Helm chart validated against Phase 1 stack on kind/minikube
- [ ] Structured logging + Prometheus metrics endpoint + scheduled audit-integrity check job

---

## Phase 2 — Oncology Expansion (Months 7–12)

### Ingestion
- [ ] FASTQ/BAM ingestion via existing Rust bioinformatics crates (e.g. `noodles`), not hand-rolled parsers
- [ ] VCF ingestion (`noodles-vcf`) for variant calls feeding VAF tracking
- [ ] Treat CellRanger/GATK outputs as pipeline inputs to ingest, not tools to reimplement
- [ ] Large-file object store handling: multipart/resumable uploads for NGS blobs
- [ ] Harmonize extension: HGVS variant nomenclature normalization; mapping to ClinVar/COSMIC identifiers

### Storage & schema — graph database introduction point
- [ ] ADR + short bake-off: ArangoDB vs Neo4j for the therapeutic-matching knowledge graph (recommendation: ArangoDB — see below); stand up chosen graph db
- [ ] Graph schema: nodes (`Gene`, `Variant`, `Drug`, `Trial`, `Pathway`); edges (`targets`, `indicated_for`, `interacts_with`)
- [ ] Document the boundary: graph db holds reference/knowledge-graph data and traversal structures; Postgres remains source of truth for patient/cohort/consent; document the sync/ETL pattern between them
- [ ] DuckDB/Parquet schema for longitudinal VAF: `variant_id, patient_id, ts, vaf, coverage_depth`

### Domain algorithms (`tpt-soma-onco`)
- [ ] VAF trend detection for MRD monitoring (statistical test for rising VAF)
- [ ] Clonal evolution model: pragmatic clone-fraction-over-time (not full phylogenetic inference)
- [ ] TME deconvolution: orchestrate an established method via Nextflow container rather than reimplementing in Rust; Rust layer stores/serves results
- [ ] Therapeutic matching engine: graph traversal query (variant → gene → pathway → drug → trial); ingest OncoKB/CIViC/ClinicalTrials.gov reference data (check OncoKB licensing terms)
- [ ] Nextflow pipeline definitions (containerized) for CellRanger/GATK/Scanpy, triggered from backend, results ingested back into Soma Core

### Security integration
- [ ] New data classes: `genomic_raw`, `genomic_variant` added to capability taxonomy
- [ ] Federated compute explicitly deferred to Phase 4 (documented, not silently dropped)
- [ ] DP extended to VAF/TME aggregate exports
- [ ] Audit ledger extended to log pipeline execution events (who triggered a Nextflow run, what data it touched)

### Frontend/API
- [ ] Multi-omics viewer: WebGL scatter/UMAP (deck.gl) for scRNA-seq, mutation timeline view
- [ ] Flight RPC extended for genomic queries (capability + DP enforced)
- [ ] Therapeutic matching UI: mutation profile → ranked therapy/trial list

### Testing/validation
- [ ] Validate against public reference datasets (1000 Genomes subset, TCGA public data) before touching real PHI
- [ ] Sparse-matrix performance testing across DuckDB/Parquet/graph db
- [ ] Cross-phase integration test: synthetic patient linked across Phase 1 clinical + Phase 2 genomic records, single combined query

---

## Phase 3 — Longitudinal Aging Engine (Months 13–18)

### Ingestion
- [ ] Methylation array ingestion (Illumina EPIC/450k `.idat`): orchestrate existing tooling (e.g. minfi via containerized pipeline); Rust ingests processed beta-value matrices
- [ ] SASP/senescence biomarker ingestion (extend lab harmonization for p16INK4a, IL-6, TNF-alpha assays)

### Domain algorithms (`tpt-soma-aevum`)
- [ ] Epigenetic clock engine: coefficient-table-driven calculators for Horvath, GrimAge, PhenoAge, DunedinPACE (data-driven so new clocks are config additions, not code)
- [ ] Validate clock outputs against published reference datasets
- [ ] Senescence burden composite scoring model from SASP markers + p16 proxy
- [ ] Longevity intervention knowledge graph: extend Phase 2 graph db with `Intervention` nodes (Rapamycin, Metformin, Acarbose, Senolytics) and edges to `Pathway`/`Biomarker`, effect sizes from ITP/TAME literature
- [ ] Cross-domain correlation queries (metabolic × aging, onco × aging) — ad hoc joins across stores this phase; full unification deferred to Phase 4 OSG

### Storage & schema
- [ ] Time-series schema for per-clock epigenetic age trajectories
- [ ] Graph schema extension for aevum intervention KG within the existing Phase 2 graph db instance (no new database technology introduced)

### Security integration
- [ ] New data class: `epigenetic_methylation`
- [ ] DP for cohort-level aging-marker aggregate exports
- [ ] Audit ledger coverage extended to aevum queries

### Frontend/API
- [ ] Aging dashboard: clock trajectories, senescence burden trend, intervention correlation explorer
- [ ] Cross-domain correlation UI (scatter/regression views spanning domains)

### Testing/validation
- [ ] Clock calculation validation suite against published reference cohorts
- [ ] Regression tests ensuring new data points correctly recompute derived aging metrics

---

## Phase 4 — Whole-Body Systemic Integration (Months 19–24+)

### Ontological Soma Graph (full)
- [ ] Design unified OSG schema: consolidate Phase 1–3 entities plus macro anatomy nodes actually needed by systemic algorithms (not exhaustive anatomy)
- [ ] Migrate/consolidate Phase 2/3 graph-db knowledge graphs + relevant Postgres relational entities into unified OSG topology
- [ ] Hybrid architecture: OSG nodes reference (not duplicate) time-series data in DuckDB/ClickHouse
- [ ] Decision point: evaluate DuckDB → ClickHouse migration if single-node embedded analytics hits scale/concurrency limits

### Simulation & Digital Twin
- [ ] Generalize Phase 1's intervention simulator into a cross-talk ODE/PDE solver framework operating over OSG edges
- [ ] Digital Twin calibration engine: fit baseline OSG parameters to a patient's multi-omics + clinical baseline, using Phase 1 intervention simulator and Phase 3 clock/senescence models as calibration targets
- [ ] Scope exact rational/fixed-point arithmetic narrowly to specific deterministic, audit-sensitive flux calculations only — general solver stays floating point
- [ ] `tpt-soma-systemic`: cross-disease shared-pathway analysis module (e.g. mTOR)
- [ ] Polypharmacy/CYP450 drug interaction engine, extending the intervention KG with CYP450 metabolism edges + interaction reference data (e.g. DrugBank)

### Sandboxed researcher compute
- [ ] Introduce Wasmtime/Wasmer sandbox for researcher-submitted analysis code (deliberately deferred to Phase 4, see Descoping)
- [ ] Define Wasm host API surface, gated through the same capability tokens used everywhere else

### 3D visualization
- [ ] Three.js/deck.gl whole-body 3D renderer mapped to OSG macro nodes
- [ ] Spatial transcriptomics rendering, extending the Phase 2 multi-omics viewer
- [ ] Interactive systemic query builder for cross-talk simulations (the spec's IGF-1/breast-tissue example)

### Federated compute
- [ ] Package core ingest+sim stack for on-prem hospital deployment reusing the Helm chart maintained since Phase 0
- [ ] Define encrypted result/weight return protocol, capability-scoped
- [ ] Pilot with a single friendly partner site rather than building a general federation framework

### Security
- [ ] Extend capability engine to express graph-traversal-scoped access (e.g. "read patient X's IGF-1 flux node but not the raw genomic node")
- [ ] DP extended to simulation-derived outputs
- [ ] Federated audit ledger reconciliation: local-site ledgers plus consistency proofs against the central ledger

### Testing/validation
- [ ] End-to-end systemic query test replicating the spec's adipose→IGF-1→breast-tissue example
- [ ] Digital twin calibration accuracy validation against held-out patient data
- [ ] Full-stack load/performance test at target scale

---

## Cross-phase dependencies

1. **Capability engine + audit ledger (Phase 0)** underpin every later phase's security — each phase *extends* the data-class taxonomy and audit coverage, never rebuilds the core crypto/ledger.
2. **Phase 1 intervention simulator** (literature-parameter PK/PD) is direct scaffolding for **Phase 4's cross-talk ODE/PDE solver and Digital Twin calibration**.
3. **Phase 2's graph db introduction** (therapeutic matching KG) is reused and extended by **Phase 3's aevum intervention KG**, then consolidated into **Phase 4's full OSG** — standing it up correctly in Phase 2 avoids a costly re-platform in Phase 4.
4. **Phase 2 multi-omics viewer** (deck.gl scatter/UMAP) is the technical foundation **Phase 4's 3D whole-body/spatial-transcriptomics viewer** extends.
5. **Phase 3 epigenetic clocks + senescence models** become calibration/validation targets for **Phase 4 Digital Twin**.
6. **DP module (Phase 0)** is extended, not rebuilt, at each phase as new aggregate export types appear (VAF/TME in Phase 2, aging markers in Phase 3, simulation outputs in Phase 4).
7. **Nextflow container orchestration pattern** established in Phase 2 (CellRanger/GATK/Scanpy) is reused in Phase 3 (methylation processing) and generalizes toward Phase 4's federated pipeline deployment.
8. **Harmonize's deterministic-mapping-table approach (Phase 1)** must be extended with new ontology mappings each phase (variant nomenclature in Phase 2, methylation probe IDs in Phase 3) before LLM-assisted fuzzy matching is worth building — treat that as a Phase 3/4 decision, not a Phase 1 blocker.
9. **Docker Compose + Helm parity maintained every phase** — Phase 4's federated compute deployment is literally "run the same Helm chart at a hospital site," so keeping the chart genuinely deployable from Phase 0 onward pays off directly in Phase 4.

## Graph database recommendation

**Introduce a dedicated graph database (ArangoDB) at the start of Phase 2**, not in Phase 1 and not deferred to Phase 3/4.

- Phase 1's relationships (patient↔cohort↔lab-result) are shallow and well served by Postgres + recursive CTEs — a graph db here would be premature infrastructure with no consuming workload.
- Phase 2's therapeutic matching is the first genuinely multi-hop traversal (variant → gene → pathway → drug → trial, ranked/filtered) where recursive CTEs become painful and a real graph engine pays for itself immediately.
- Prefer **ArangoDB over Neo4j**: multi-model (its document store naturally absorbs imported knowledge-base reference data like OncoKB/CIViC alongside the graph), AQL handles the needed traversal patterns, and its open-source edition self-hosts cleanly under both Docker Compose and Helm without Neo4j Community vs Enterprise clustering/licensing complications. Do a short ADR bake-off before Phase 2 kickoff (check Rust driver maturity for both at that time) rather than treating this as a hard lock.
- Once introduced in Phase 2, the same graph db instance/pattern is reused (not replaced) for Phase 3's aevum intervention KG and consolidated into Phase 4's full OSG.

## Descoping decisions (pragmatic substitutes — none of these drop security/compliance requirements)

| Spec ambition | Why it's unrealistic for solo/small team now | Pragmatic substitute |
|---|---|---|
| `no_std` zero-allocation parsers for every ingest format | FHIR/FASTQ/BAM are inherently variable-length/dynamic; no embedded-device deployment target in sight | Std Rust parsers, well-tested; avoid unnecessary allocation as internal discipline, not a hard requirement |
| Full automated federated compute (auto-deploy to hospital infra, automated encrypted weight return) | A generalized federation framework is a multi-year systems project on its own | Deferred to Phase 4 as a single manually-orchestrated pilot deployment of the existing Helm chart at one partner site |
| LLM-assisted fuzzy ontology matching (Phase 1, per spec) | Adds an LLM-in-the-loop dependency to a Phase 1 blocker workstream already carrying custom crypto | Deterministic mapping tables + human-in-the-loop manual review for Phase 1–2; revisit once mapping volume becomes the bottleneck (~Phase 3/4) |
| Exact rational/fixed-point arithmetic across the whole simulation engine | Performance-prohibitive for general ODE/PDE solving; most literature PK/PD models are already floating-point/statistical | Scope exact arithmetic narrowly to specific deterministic, audit-sensitive calculations only (Phase 4); general solver stays floating point |
| "Custom Rust-native graph database (or heavily optimized ArangoDB)" | Building a graph database from scratch is one of the highest-risk build-vs-buy calls in the spec | Use ArangoDB directly (already effectively decided per the recommendation above) |
| TME deconvolution / epigenetic clocks / methylation processing reimplemented from scratch | Reinventing peer-reviewed statistical/ML methods is high validation risk and slow for a small team | Orchestrate established, validated tools via Nextflow containers; Rust layer handles orchestration, storage, and security only |
| Wasm sandboxed compute for arbitrary researcher code (implied as core architecture from the start) | Building/auditing a safe arbitrary-code execution boundary competes for the same limited bandwidth as CBAC/audit work | Phases 1–3 use trusted, vetted Nextflow containers; Flight RPC + Jupyter already lets researchers run exploratory code client-side; introduce Wasm sandboxing in Phase 4 |
| Full whole-body OSG (every organ/tissue/cell) from the outset | No consuming algorithm needs most of that model until Phase 4 | Model only the entities the active phase's domain algorithms actually consume |

## Critical first files

These are the first files to create — everything else in the roadmap depends on them:

- `Cargo.toml` — workspace root, defines crate boundaries for the whole monorepo
- `crates/tpt-soma-capability/src/lib.rs` — custom capability-token cryptosystem (Phase 1 blocker, used by every later phase)
- `crates/tpt-soma-audit/src/lib.rs` — append-only hash-chained audit ledger (Phase 1 blocker, used by every later phase)
- `deploy/docker-compose.yml` and `deploy/helm/tpt-soma/` — dual deployment path, kept in parity every phase, culminating in Phase 4's federated deployment
- `schemas/` — shared Arrow/Protobuf schema definitions establishing data-contract conventions every ingest/storage/API component depends on
