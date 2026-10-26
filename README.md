# tpt-soma

A multi-scale computational physiology platform for modelling whole-body human physiology as a set of coupled, non-linear systems, with capability-based access control and an append-only audit trail built in from the start.

The architecture, scales and roadmap are described in [spec.txt](spec.txt) (SDD v2.0.0). Current progress against the four-phase roadmap is tracked in [TODO.md](TODO.md).

## Layout

tpt-soma is a single Cargo workspace (`crates/`) plus a React/TypeScript frontend (`frontend/`), deployment manifests (`deploy/`) and documentation (`docs/`).

| Crate | Purpose |
| --- | --- |
| [tpt-soma-core](crates/tpt-soma-core) | Connection pooling, SQL migrations, query helpers, and the differential-privacy foundation. |
| [tpt-soma-capability](crates/tpt-soma-capability) | Custom Ed25519-signed capability tokens for cohort- and data-class-scoped access. |
| [tpt-soma-audit](crates/tpt-soma-audit) | Append-only, hash-chained compliance audit ledger with integrity alerting. |
| [tpt-soma-ingest](crates/tpt-soma-ingest) | Ingestion parsers for VCF and AnnData/h5ad, with checksum-on-write and quarantine. |
| [tpt-soma-harmonize](crates/tpt-soma-harmonize) | Deterministic ontology mapping (dbSNP rsID, HGNC, LOINC, SNOMED) with human-in-the-loop review. |
| [tpt-soma-genomica](crates/tpt-soma-genomica) | Genomic variant annotation and harmonization. |
| [tpt-soma-cytos](crates/tpt-soma-cytos) | Single-cell RNA-seq (10x/AnnData) orchestration and storage. |
| [tpt-soma-organon](crates/tpt-soma-organon) | Organ function, FHIR/CSV ingestion, imaging metadata and clinical calculators. |
| [tpt-soma-chronos](crates/tpt-soma-chronos) | Time-series and CGM engine: resampling, glycemic variability, longitudinal trajectories. |
| [tpt-soma-simulacrum](crates/tpt-soma-simulacrum) | ODE/PDE solvers, PK/PD models, digital-twin calibration and the OSG cross-talk solver. |
| [tpt-soma-pathos](crates/tpt-soma-pathos) | Computational pathology: metabolic/endocrine, oncology, longevity and cardiovascular models. |
| [tpt-soma-clinica](crates/tpt-soma-clinica) | Clinical trial design, cohort discovery, and biomarker discovery/validation statistics. |
| [tpt-soma-flight](crates/tpt-soma-flight) | In-house columnar batches, Arrow-compatible IPC and Flight gRPC. |
| [tpt-soma-sandbox](crates/tpt-soma-sandbox) | Capability-gated host API surface for researcher-submitted WASM compute. |
| [tpt-soma-api](crates/tpt-soma-api) | HTTP and Arrow Flight API server with capability, audit and differential-privacy enforcement. |

## Prerequisites

- Rust 1.97.0 (pinned in [rust-toolchain.toml](rust-toolchain.toml); installed automatically by `rustup`)
- Docker and Docker Compose, for the containerised stack
- Node.js, for the frontend

## Building and testing

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo deny check
```

These are the same checks CI runs in [.github/workflows/ci.yml](.github/workflows/ci.yml). [.pre-commit-config.yaml](.pre-commit-config.yaml) mirrors them locally.

## Running the stack

The Docker Compose deployment starts Keystone, MinIO, the API and the frontend:

```sh
cp deploy/.env.example deploy/.env   # set the bootstrap credentials
docker compose -f deploy/docker-compose.yml up -d
```

A Helm chart skeleton for Kubernetes is in [deploy/helm/tpt-soma](deploy/helm/tpt-soma). See [ADR 004](docs/adr/004-dual-deployment.md) for the dual-deployment policy.

## Frontend

```sh
cd frontend
npm install
npm run dev
```

## Documentation

- [spec.txt](spec.txt): system design document
- [TODO.md](TODO.md): roadmap and status
- [docs/adr/](docs/adr/): architecture decision records
- [docs/runbooks/](docs/runbooks/): onboarding and operational runbooks
- [docs/security/](docs/security/) and [docs/testing/](docs/testing/): security and test notes
- [docs/irb/](docs/irb/): IRB material

## Security and data handling

Access to cohort data is granted only through signed capability tokens scoped to a cohort and data class. Every access is recorded in the hash-chained audit ledger. Real patient data must not be loaded until the [real PHI onboarding gate](docs/runbooks/real-phi-onboarding-gate.md) has been passed.

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
