# Data Classification & Flow

## Data inventory

| Data class | Sensitivity | Phase | Examples |
|---|---|---|---|
| `genomic_raw` | Restricted | 1 | Raw sequencing reads (VCF/FASTQ blobs in object store) |
| `genomic_variant` | Confidential | 1 | Variant calls (VCF), dbSNP-annotated |
| `transcriptomic_scrna` | Confidential | 1 | Single-cell RNA-seq counts, UMAP, cluster labels |
| `phi_demographic` | Restricted | 1 | PHI demographic data |
| `clinical_observation` | Confidential | 2 | Organ function panels (LOINC-coded), FHIR observations |
| `cgm_continuous` | Confidential | 2 | Continuous glucose monitor time series |
| `organ_imaging` | Restricted | 2 | Imaging pixel data (object store) + DICOM metadata |
| `simulation_output` | Confidential | 3 | Digital twin / PK-PD simulation trajectories |
| `dp_budget` | Internal | 0 | DP epsilon spend records (audit) |

Sensitivity semantics: `Public` (no restriction) < `Internal` (employee-only)
< `Confidential` (research-access-only, CBAC-gated) < `Restricted`
(de-identified-or-consented PHI, tightest CBAC scope, IRB oversight).

The taxonomy lives in two places, kept in sync:

- `tpt-soma-capability/src/registry.rs` (`seed_phase0`, `seed_phase2`)
- `data_class_registry` table (seeded by migrations)

## Storage layout

- **Relational data** — Keystone (Postgres-wire-compatible):
  - `samples`, `cohorts`, `cohort_membership` (Phase 1)
  - `variants`, `sample_variants`, `scrna_expression`, `scrna_umap` (Phase 1)
  - `organ_function_observations`, `cgm_readings`, `fhir_resource_payloads`,
    `organ_imaging_records` (Phase 2)
  - `audit_ledger` (Phase 0)
  - Plexus graph: `Gene`/`Variant`/`ProteinInteraction` nodes (Phase 1),
    `Organ`/`OrganSystem` nodes + coupling edges (Phase 2)
- **Object storage (S3/MinIO)** — raw blobs referenced by URI:
  - `raw-omics` bucket: raw VCF/AnnData files
  - `raw-omics-quarantine`: malformed uploads held for review
  - imaging pixel data
- **Time series** — Keystone Chronos extension (`cgm_readings`,
  `organ_function_observations` series)

## Data flow

1. **Ingest** (upload endpoint) → validation → checksum-on-write to MinIO →
   parse → normalized rows in Keystone. Malformed files go to the quarantine
   bucket.
2. **Harmonization** → deterministic mapping tables (dbSNP, HGNC, LOINC,
   SNOMED, UBERON); unmapped identifiers go to a human-in-the-loop review
   queue.
3. **Storage** → relational + graph + time-series rows keyed by
   `sample_id`/`subject_id`, with cohort membership linking samples to
   cohorts.
4. **Access** → CBAC token check → audit write → query executes. Cohort-level
   aggregates additionally pass through the DP module.
5. **Analysis** → Scanpy (containerized) reads expression data, writes back
   UMAP/clusters; researchers query via HTTP API or Arrow Flight RPC.

## Privacy-relevant invariants

- No raw PHI values are written into the audit ledger.
- Cohort aggregate exports never expose true counts without DP noise.
- Raw data access is always per-token, scoped, expired, revocable, and
  audited.
