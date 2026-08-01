# tpt-soma — Project TODO

Tracking checklist for the full 4-phase roadmap defined in `spec.txt` (v2.0.0). Scope decisions locked in for this checklist: solo/small team; storage fully consolidated on the sibling `tpt-keystone-db` project (relational + Plexus graph + Chronos time-series extensions) instead of DuckDB/Postgres/a dedicated graph DB, with raw omics/imaging blobs kept in S3-compatible object storage (MinIO) referenced by URI; custom capability-token cryptosystem and custom hash-chained audit ledger (both independent of `tpt-archon` and of Keystone's Mirror component — neither sibling project was a fit for researcher-facing access control/audit, so both are built internally); differential privacy is a Phase 0 blocker; the ingestion/security stack is source-agnostic from day one so it can take real PHI or public reference data through the same code path (Phase 1 itself validates against public/open-consent datasets to move fast without IRB overhead); dual Docker Compose + Kubernetes/Helm deployment maintained from day one; single Cargo-workspace monorepo; OSS project scaffolding excluded for now; `tpt-cerebrum` integration is aspirational and out of scope beyond a single Phase 4 note.

---

## Phase 0 — Monorepo Foundational Setup (blocks all of Phase 1)

### Repo & workspace layout
- [ ] `git init` (already done), `.gitignore` (already present — verify it covers Rust `target/`, Node `node_modules/`, `.env`, MinIO local volumes, Keystone `tpt-data/`)
- [ ] Cargo workspace root `Cargo.toml` with member crates: `tpt-soma-ingest`, `tpt-soma-harmonize`, `tpt-soma-core` (Keystone data-access layer: connection pooling, schema migrations, query helpers over `sqlx`/`tokio-postgres`), `tpt-soma-capability`, `tpt-soma-audit`, `tpt-soma-genomica`, `tpt-soma-cytos`, `tpt-soma-api` (placeholders for `tpt-soma-organon`, `tpt-soma-chronos`, `tpt-soma-simulacrum`, `tpt-soma-pathos`, `tpt-soma-clinica` added in later phases, matching spec §5's module names exactly)
- [ ] `rust-toolchain.toml` pin, workspace `rustfmt.toml`, `clippy.toml`, `deny.toml` (cargo-deny for license/advisory scanning)
- [ ] `frontend/` package: Vite + React + TypeScript scaffold
- [ ] `schemas/` directory: shared Arrow schema definitions + Protobuf files, with a documented versioning/compatibility policy (additive-only within major version)
- [ ] `docs/adr/` directory + first ADRs: (1) full storage consolidation on `tpt-keystone-db` over DuckDB+Postgres+dedicated-graph-db, connected via standard Postgres-wire client rather than `tpt-keystone-sdk`, (2) custom CBAC over Biscuit/Macaroons/`tpt-archon`'s kernel-level capabilities, (3) custom audit ledger over Keystone's Mirror component, (4) dual Compose/Helm deployment, (5) schema evolution policy

### CI / dev tooling
- [ ] CI pipeline (build/test/lint matrix): Rust workspace (`cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`) and frontend (`tsc`, `eslint`, `vitest`/`jest`)
- [ ] Pre-commit hook config mirroring CI checks locally
- [ ] Versioning convention decision (workspace-wide vs per-crate) documented in ADR

### Deployment skeletons (dual-path from day one)
- [ ] `deploy/docker-compose.yml`: `keystone` service (built from `tpt-keystone-db`'s `tpt-keystone/Dockerfile`, or a pinned image once one is published — Postgres wire on 5432, HTTP/JSON bridge on 5435, metrics on 9187), `minio` (S3-compatible object storage), `api` service placeholder, `frontend` dev service, volumes for Keystone's `tpt-data/` and MinIO
- [ ] Bootstrap credentials for Keystone's compose deployment: `TPT_AUTH_BOOTSTRAP_USER`/`TPT_AUTH_BOOTSTRAP_PASSWORD` via `.env` (Keystone's compose refuses to start without these — it does not default to zero-config auth when bound to `0.0.0.0`)
- [ ] `deploy/docker-compose.override.yml` for local dev conveniences (hot reload, exposed debug ports)
- [ ] `deploy/helm/tpt-soma/` chart skeleton: `Chart.yaml`, `values.yaml`, templates mirroring compose services 1:1 (`keystone`, `minio`, `api`, `frontend`) — must be kept genuinely deployable every phase, not bolted on later
- [ ] Secrets strategy for both paths: `.env.example` for Compose; K8s `Secret`/`values-secrets.yaml.example` for Helm (sealed-secrets/SOPS noted as future hardening)
- [ ] Dockerfiles: multi-stage Rust build, frontend static build, shared image family used by both deploy paths

### Capability-token cryptosystem (custom build, Phase 1 blocker)
- [ ] ADR: token design — custom Ed25519-signed capability tokens with HMAC-chained caveats for attenuation, short expiry + refresh
- [ ] `tpt-soma-capability` crate: token struct (subject, resource class, cohort scope, action, expiry, nonce), issuance API, verification function, attenuation (derive narrower child token without re-signing by root key)
- [ ] Initial resource/data-class taxonomy (extensible registry, stored as a `tpt-soma-core` table in Keystone) — Phase 1 needs `genomic_raw`, `genomic_variant`, `transcriptomic_scrna`, `phi_demographic` (reserved from day one even though Phase 1 validation primarily uses public data)
- [ ] Root signing key lifecycle: dev = local keyfile; abstracted behind a trait so prod can later swap in KMS/HSM
- [ ] Revocation mechanism (revocation list keyed by nonce, or short TTL + no revocation for v1 — pick one, document trade-off)
- [ ] Unit tests: forged signature rejected, expired token rejected, attenuated token cannot exceed parent scope, revoked token rejected
- [ ] CLI/admin script to issue a capability token for a named researcher/cohort/data-class

### Audit ledger (custom build, Phase 1 blocker)
- [ ] `tpt-soma-audit` crate: append-only Keystone table with hash-chaining (`row_hash = H(prev_row_hash || event_payload)`)
- [ ] Audit event schema: actor, resource/data-class, action, cohort/sample scope, timestamp, query fingerprint, outcome — no raw PHI values in the ledger
- [ ] Single choke-point write path: audit logging happens inside capability-verification middleware, not scattered per-endpoint
- [ ] Chain-integrity verification job (recompute and compare hash chain; alert on mismatch)
- [ ] Compliance report generator (CLI: "show all access to cohort X in date range")

### Differential privacy foundation
- [ ] DP module: Laplace mechanism for count/sum/mean aggregates, configurable epsilon
- [ ] Per-cohort/per-dataset epsilon budget tracker, spend recorded through the audit ledger
- [ ] Single "cohort aggregate export" enforcement code path that all future domain modules must call through
- [ ] Tests: noise-injection statistical sanity checks; budget exhaustion blocks further exports

### `tpt-soma-core` (Keystone data-access layer)
- [ ] Connection pooling + migration runner over Keystone's Postgres wire protocol (`sqlx` or `tokio-postgres`)
- [ ] Schema migration tooling (plain SQL migrations, applied at startup/CI, versioned in `tpt-soma-core/migrations/`)
- [ ] Thin query-builder helpers for Plexus graph queries (`MATCH` pattern statements, `graph_neighbors()`/`graph_bfs()` table functions) alongside standard SQL, so callers aren't hand-writing raw SQL for graph traversal
- [ ] Object-store client wrapper (MinIO/S3) for raw blob upload/download with checksum-on-write, used by `tpt-soma-ingest`

---

## Phase 1 — Molecular & Cellular Wedge (Months 1–6)

Focus: `tpt-soma-genomica` and `tpt-soma-cytos`, narrowed to a realistic slice (see Descoping table) — VCF variant ingestion, single-cell RNA-seq (10x/AnnData), and a minimal multi-omics join. GWAS, methylation/chromatin accessibility, bulk/mass-spec proteomics, metabolomics/lipidomics, microbiomics, spatial biology, digital pathology, and cell-cell communication modeling stay under these module labels but are explicitly deferred past Phase 1.

### Ingestion
- [ ] `tpt-soma-ingest`: VCF parser using `noodles-vcf` (reused, not reimplemented)
- [ ] `tpt-soma-ingest`: AnnData/`.h5ad` parser for 10x Genomics CellRanger scRNA-seq output
- [ ] Upload/ingest endpoint with validation + quarantine bucket for malformed files, source-agnostic (accepts a public-reference-dataset sample or a real patient sample identically, gated only by the capability token presented)
- [ ] `tpt-soma-harmonize`: deterministic mapping table for variant identifiers (dbSNP rsID, ClinVar) and gene symbols (HGNC)
- [ ] Harmonize: human-in-the-loop review CLI/UI for unmapped identifiers (LLM-assist deferred, see Descoping)
- [ ] Golden-file tests using public reference data: 1000 Genomes subset VCFs, 10x public PBMC (3k/10k) AnnData sample

### Storage & schema (Keystone)
- [ ] Relational tables: `samples` (sample_id, patient_id nullable for public/de-identified data, source = `public`|`patient`, dataset provenance), `cohorts`, `cohort_membership`, `data_class_registry`
- [ ] Plexus graph schema: `Gene`, `Variant`, `ProteinInteraction` nodes; `harbors_variant`, `interacts_with` edges — first genuinely graph-shaped Phase 1 data (protein-protein interaction network)
- [ ] Table for scRNA-seq expression matrices (sparse storage: `sample_id, cell_id, gene_id, count` or a Parquet-in-object-store + pointer-row pattern if row-per-count is too dense — benchmark both before committing)
- [ ] MinIO bucket layout for raw VCF/AnnData files + checksum-on-write
- [ ] Dev backup/restore scripts for Keystone (`tpt-data/` volume) + MinIO bucket

### Domain algorithms (`tpt-soma-genomica`, `tpt-soma-cytos`)
- [ ] Variant harmonization + basic annotation (rsID/ClinVar lookup) pipeline
- [ ] scRNA-seq preprocessing orchestrated through a single containerized Scanpy script (normalization, PCA, UMAP, Leiden clustering) — not reimplemented in Rust, consistent with "orchestrate established tools, Rust stores/serves results"; formal Nextflow orchestration arrives in Phase 2
- [ ] `tpt-soma-cytos`: ingest Scanpy's output (UMAP coordinates + cluster labels) into Keystone, keyed by `sample_id`/`cell_id`
- [ ] Minimal multi-omics integration query: join variant presence + expression level by `sample_id`, proving the OSG "linked nodes" concept without a full knowledge graph yet
- [ ] Unit tests for harmonization mapping correctness against known reference variants/genes

### Security integration
- [ ] Wire capability check + audit write into every genomica/cytos query endpoint
- [ ] Cohort aggregate endpoints routed through the DP module
- [ ] Pilot researcher onboarding: token issuance workflow for a small initial researcher cohort
- [ ] IRB documentation package describing CBAC/DP/audit mechanisms (non-code deliverable, required before any real PHI/patient sample flows — Phase 1 itself can proceed on public data without it)
- [ ] Internal threat-model review + basic self-pen-test checklist before any real patient sample onboarding

### Frontend/API
- [ ] Arrow Flight RPC service (`arrow-flight`) exposing genomica/cytos queries to Jupyter/RStudio — Keystone doesn't provide Flight natively (it has Postgres-wire, HTTP/JSON, WebSocket/gRPC streaming, and MCP), so this is genuine new work: query Keystone, serialize results as Arrow record batches, stream via Flight
- [ ] Minimal API for the web frontend (capability token as bearer credential)
- [ ] React/TS: sample/cohort selector, deck.gl UMAP/scatter viewer for scRNA-seq clusters, variant table view
- [ ] JupyterLab smoke test: `pyarrow.flight` client against the Flight service
- [ ] Simple admin-issued token flow for researcher login (SSO/OAuth deferred)

### Testing/validation
- [ ] Unit test coverage targets for ingest/harmonize/genomica/cytos crates
- [ ] End-to-end integration test: raw VCF + AnnData file → stored in Keystone → variant/expression joined → queryable via Flight
- [ ] Keystone load test at realistic scale (sparse scRNA-seq matrix, ~thousands of cells × thousands of genes × N samples)
- [ ] Security tests: unauthorized query rejected + logged; tampered audit chain detected
- [ ] Pilot cohort onboarding runbook + rollback plan (covering both a public-dataset pilot and, if/when available, a first real-patient sample)

### Deployment
- [ ] Docker Compose fully wired for Phase 1 stack (keystone, minio, api, flight server, frontend)
- [ ] Helm chart validated against Phase 1 stack on kind/minikube
- [ ] Structured logging + Prometheus metrics endpoint + scheduled audit-integrity check job

---

## Phase 2 — Physiological & Temporal Expansion (Months 7–12)

Focus: `tpt-soma-organon` and `tpt-soma-chronos`. This is where the old roadmap's metabolic/CGM work now lands, and where real clinical/EHR PHI is exercised in practice for the first time (the security stack has been ready since Phase 0).

### Ingestion
- [ ] FHIR R5 subset parser: `Patient`, `Observation` (organ function panels, lipids, HbA1c, hs-CRP LOINC codes)
- [ ] Dexcom binary stream parser; Libre binary stream parser
- [ ] Organ imaging ingestion: MRI/CT/ultrasound/PET metadata + blob storage (imaging pixel data in MinIO, DICOM metadata in Keystone)
- [ ] CSV/manual upload ingestion path for organ function panels (avoids blocking pilot onboarding on full EHR/FHIR integration per source hospital)
- [ ] `tpt-soma-harmonize` extension: LOINC/SNOMED/UBERON mapping for organ-system observations

### Storage & schema
- [ ] Keystone Chronos extension: `cgm_readings(sample_id, ts, glucose_mgdl, source)`, longitudinal organ-function-test trajectories, gap-filling/resampling support
- [ ] Plexus graph extension: `Organ`, `OrganSystem` nodes; `cross_organ_coupling` edges (function/dysfunction cascades)
- [ ] Document Keystone's Canopy (JSON) extension usage for storing raw FHIR resource payloads alongside normalized relational rows

### Domain algorithms (`tpt-soma-organon`, `tpt-soma-chronos`)
- [ ] 5-minute interval resampling / gap-filling logic for continuous sensor data
- [ ] TIR / TBR / TAR, CV, MAGE calculations (glycemic variability, now under `chronos`/`organon` rather than the old `tpt-soma-metabolic`)
- [ ] Organ function test calculators: ejection fraction, GFR, pulmonary function indices, liver enzyme panel interpretation
- [ ] Circadian/ultradian rhythm analysis (oscillation detection over 24h and shorter cycles)
- [ ] Clinical reference ranges stored as versioned config data, not hardcoded constants

### Security integration
- [ ] New data classes: `clinical_observation`, `cgm_continuous`, `organ_imaging`
- [ ] Real-PHI pilot: first patient-linked cohort onboarded end-to-end through the capability/audit/DP stack built in Phase 0
- [ ] Audit ledger extended to cover imaging access + FHIR ingestion events

### Frontend/API
- [ ] Flight RPC extended for organon/chronos queries
- [ ] React/TS: longitudinal trajectory charts (CGM with TIR bands), organ function dashboards
- [ ] Cross-phase integration test: a sample linked across Phase 1 genomic/cytos records and Phase 2 clinical records, single combined query

### Testing/validation
- [ ] Golden-file tests for FHIR bundles, Dexcom/Libre sample exports
- [ ] Keystone Chronos load test at realistic longitudinal scale (~10^5 points/patient/year × N patients × years)
- [ ] Real-PHI onboarding runbook, informed by the Phase 1 pilot runbook

### Deployment
- [ ] Docker Compose/Helm updated for Phase 2 ingestion services

---

## Phase 3 — Computational & Digital Twin Core (Months 13–18)

Focus: `tpt-soma-simulacrum`.

### Domain algorithms
- [ ] Rust-based ODE/PDE solver framework for metabolic pathway and signaling-cascade models
- [ ] PK/PD modeling (absorption/distribution/metabolism/excretion)
- [ ] Differentiable physiology: PyTorch/JAX bridge so researchers can fit model parameters to empirical data via gradient descent
- [ ] Scope exact rational/fixed-point arithmetic narrowly to specific deterministic, audit-sensitive flux calculations only — the general solver stays floating point (see Descoping)
- [ ] Digital Twin calibration MVP: fit baseline model parameters to a sample's Phase 1/2 multi-omics + clinical baseline

### Storage & schema
- [ ] Keystone schema for simulation run metadata, parameter sets, and calibration targets
- [ ] Simulation outputs stored via Chronos (trajectories) and Plexus (which OSG edges/nodes a simulation touched)

### Security integration
- [ ] New data class: `simulation_output`
- [ ] DP extended to simulation-derived aggregate exports

### Frontend/API
- [ ] Simulation configuration UI + results visualization (trajectory plots, parameter sensitivity views)
- [ ] Flight RPC extended for simulation queries

### Testing/validation
- [ ] Solver correctness tests against known analytical/published reference solutions
- [ ] Digital Twin calibration accuracy validation against held-out sample data

---

## Phase 4 — Translational Pathology & Clinical Integration (Months 19–24+)

Focus: `tpt-soma-pathos` and `tpt-soma-clinica`.

### Domain modules (`tpt-soma-pathos`)
- [ ] Metabolic & Endocrine (diabetes): insulin resistance modeling, metabolic syndrome, building on Phase 2's CGM/chronos work
- [ ] Oncology: solid tumors, hematologic malignancies, tumor microenvironment, immunotherapy response, building on Phase 1's genomic variant work
- [ ] Longevity & Aging: epigenetic clocks, senescence tracking, age-related disease
- [ ] Cardiovascular, autoimmune, infectious sub-modules

### Domain modules (`tpt-soma-clinica`)
- [ ] EHR & FHIR integration hardened beyond Phase 2's subset (full resource coverage as needed)
- [ ] Clinical trial design & management: cohort discovery, patient recruitment, protocol design, adverse event tracking
- [ ] Biomarker discovery & validation statistical pipelines
- [ ] Real-World Evidence (RWE) analytics on observational datasets

### Ontological Soma Graph consolidation
- [ ] Consolidate Phase 1–3 entities plus the macro anatomy nodes actually needed by pathos/clinica algorithms (not exhaustive anatomy) into a unified OSG topology within Keystone's Plexus extension
- [ ] Generalize Phase 3's ODE/PDE framework into a cross-talk solver operating over full OSG edges (the spec's adipose→IGF-1→breast-tissue example)

### Sandboxed researcher compute
- [ ] Introduce Wasm sandboxing for researcher-submitted analysis code — revisit whether Keystone's existing `wasmtime`-sandboxed WASM UDFs can be reused directly instead of building a separate sandbox
- [ ] Define Wasm host API surface, gated through the same capability tokens used everywhere else

### 3D visualization
- [ ] Three.js/deck.gl whole-body 3D renderer mapped to OSG macro nodes
- [ ] Interactive systemic query builder for cross-talk simulations

### Federated compute
- [ ] Package core ingest+sim stack for on-prem deployment reusing the Helm chart maintained since Phase 0
- [ ] Define encrypted result/weight return protocol, capability-scoped
- [ ] Pilot with a single friendly partner site rather than building a general federation framework

### Security
- [ ] Extend capability engine to express graph-traversal-scoped access (e.g. "read a sample's IGF-1 flux node but not its raw genomic node")
- [ ] DP extended to simulation-derived and cross-domain outputs
- [ ] Federated audit ledger reconciliation: local-site ledgers plus consistency proofs against the central ledger

### Future work (not scoped in this checklist)
- [ ] `tpt-cerebrum` integration: connecting neurological data to enable whole-body queries (e.g. systemic insulin resistance → cerebral glucose metabolism) is explicitly aspirational for this roadmap — revisit scoping once `tpt-soma`'s own OSG is consolidated and `tpt-cerebrum` is further along

### Testing/validation
- [ ] End-to-end systemic query test replicating the spec's adipose→IGF-1→breast-tissue example
- [ ] Full-stack load/performance test at target scale

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
