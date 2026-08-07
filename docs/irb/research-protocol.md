# Research Protocol Overview

This document frames how the platform is intended to be used for research and
what participant protections the platform enforces. It is a narrative for IRB
submission and is not a substitute for a study-specific protocol.

## Intended research use

The platform enables researcher access to multi-omics and clinical data for:

- Variant–expression association exploration (Phase 1).
- Single-cell transcriptomic characterization (Phase 1).
- Longitudinal physiological trajectories (CGM, organ function tests) and
  glycemic variability analysis (Phase 2).
- Digital twin / PK-PD simulation and calibration (Phase 3).
- Translational disease modeling across metabolic, oncology, and other
  domains (Phase 4).

## Data minimization

- Ingestion is scoped to specific data types (VCF variants, scRNA-seq,
  selected clinical observations, CGM readings) rather than bulk wholesale data
  capture.
- Raw blobs are stored in private object storage referenced only by URI; the
  relational layer stores derived, normalized values.
- The audit ledger stores no raw PHI values.

## Participant protections enforced by the platform

1. **Least privilege access** — researchers receive narrowly scoped,
   expiring, revocable capability tokens (see CBAC doc). No blanket
   read-everything credentials.
2. **Per-cohort scoping** — a token's `cohort_scope` limits which cohorts it
   can touch; tokens are issued per cohort for the pilot.
3. **Differential privacy on aggregates** — cohort-level exports are noise
   injected against a per-cohort epsilon budget, limiting individual-level
   inference from aggregate outputs.
4. **Full audit trail** — every access is recorded in a tamper-evident,
   hash-chained ledger (see Audit Ledger doc).
5. **Quarantine of malformed data** — malformed uploads are isolated, not
   silently dropped or partially persisted.
6. **Human-in-the-loop harmonization** — unmapped identifiers are reviewed by
   a human before they enter derived datasets, avoiding silent misannotation.

## Consent and authorization model

- The pilot operates on **public or open-consent datasets** (Phase 1
  validation) and, only after IRB approval and the threat-model review is
  complete, on **consented, patient-linked data** (Phase 2).
- Access tokens are issued only for cohorts for which the researcher is
  authorized; authorization is granted by the platform administrator, not by
  the researcher.

## Researcher responsibilities

- Use only issued tokens within their declared scope.
- Route all cohort aggregate queries through the platform's aggregate export
  endpoints (the only path that applies DP).
- Report any suspected data exposure or token compromise per the incident
  response steps in the pilot onboarding runbook.
- Use Flight RPC and Jupyter only with platform-issued credentials once
  Flight authentication is enabled (see threat model TM-02).

## Alignment to regulations

The controls described in this package are designed to support compliance
obligations common to research under HIPAA (access logs, audit, minimal data)
and GDPR (data minimization, access control, accountability). Specific
determinations (e.g. de-identification status, lawful basis) are made per study
by the responsible IRB and privacy office.
