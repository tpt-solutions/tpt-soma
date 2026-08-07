# Real-PHI Cohort Onboarding Runbook + Rollback Plan

Companion to [`pilot-cohort-onboarding.md`](pilot-cohort-onboarding.md). This
runbook covers the **patient-linked (real PHI)** onboarding path that the Phase 1
public/open-consent pilot deliberately avoids. It is the gate for the Phase 2
real-PHI pilot (TODO line 146) and closes the open TODO item at line 135.

> This is a procedure document, not legal/regulatory advice. It assumes an IRB
> (or equivalent ethics committee) approval already exists for the cohort.

## Hard prerequisites (do not skip)

- [ ] **IRB / ethics approval** granted for the specific cohort + data classes.
- [ ] [`../irb/README.md`](../irb/README.md) package current and attached to the submission.
- [ ] [`../security/threat-model.md`](../security/threat-model.md) sign-off gate closed
      (all Critical/High findings closed or accepted with documented mitigations).
- [ ] [`../security/self-pentest-checklist.md`](../security/self-pentest-checklist.md)
      executed and recorded against the live deploy that will hold PHI.
- [ ] `TPT_ENFORCE_SECRETS=1` set in the PHI deployment (refuses ephemeral signing keys / defaults).
- [ ] Capability root signing key provisioned via a real KMS/HSM-backed backend
      (`tpt-soma-capability::signing::KmsSigningBackend`), not the dev local keyfile.
- [ ] Revocation persistence (TM-03) addressed, OR a documented restart-revoke
      procedure with bounded token lifetime (<= 1h) is in force.
- [ ] Audit append failures surfaced (TM-06) or a monitored dead-letter queue exists.

## Data classes in scope

`genomic_raw`, `genomic_variant`, `transcriptomic_scrna`, `phi_demographic`,
`clinical_observation`, `cgm_continuous`, `organ_imaging`. Tokens must carry the
minimum set a researcher needs (data minimization — see `irb/data-classification.md`).

## Step 1 — Provision secrets (PHI-grade)

```bash
# non-dev: enforce secret checking and mount the KMS-backed key
export TPT_ENFORCE_SECRETS=1
# key arrives via the KmsSigningBackend; CAPABILITY_ROOT_KEY_PATH still points at
# the mounted secret material (Compose: capability-secrets volume; Helm: secret).
```

Never commit keys. The PHI deploy must use mounted secrets, not plaintext env.

## Step 2 — Cohort + consent linkage

```sql
INSERT INTO cohorts (id, name, description, is_public)
VALUES ('phi-cohort-x', 'Real-PHI Cohort X', 'IRB #2026-...', false);

-- patient_id is NEVER the MRN; map via a one-way pseudonymization table
-- held outside tpt-soma (the platform stores patient_id nullable + subject_id).
INSERT INTO cohort_membership (cohort_id, sample_id, subject_id)
VALUES ('phi-cohort-x', 'sample-...', 'subject-...');
```

Document the consent scope (which data classes, which analyses) in the cohort
register; the capability tokens issued in Step 4 must not exceed it.

## Step 3 — Issue scoped researcher tokens

Issue the **narrowest** token that covers the approved work:

```bash
cargo run -p tpt-soma-api --bin tpt-soma-admin -- issue \
  --subject researcher-2 \
  --resource-class genomic_variant \
  --action read \
  --cohort phi-cohort-x \
  --expiry 3600 \
  --key /run/secrets/capability_root_key
```

For aggregate/export use, issue a separate `export`-action token scoped to the
cohort; never hand a researcher `admin`.

## Step 4 — Ingest PHI through the same code path as reference data

The ingestion/security stack is source-agnostic (Phase 1 design): PHI and public
data go through the identical VCF/h5ad/FHIR/CGM/imaging endpoints.

```
curl -H "Authorization: Bearer $TOKEN" -F "file=@patient.vcf" \
  http://localhost:8080/api/v1/ingest/vcf
curl -H "Authorization: Bearer $TOKEN" -F "file=@patient.h5ad" \
  http://localhost:8080/api/v1/ingest/h5ad
curl -H "Authorization: Bearer $TOKEN" -F "file=@observations.csv" \
  http://localhost:8080/api/v1/ingest/organ-csv
```

- Verify blobs land in MinIO `raw-omics` (or `organ_imaging`) with checksums.
- Verify parsed rows land in the normalized tables; nothing in quarantine.
- Confirm `phi_demographic` values are pseudonymized before insert.

## Step 5 — Verification (acceptance)

1. `GET /api/v1/variants/<sample_id>` returns expected variants (token scoped).
2. `POST /api/v1/join/variant-expression` returns joined rows for the cohort.
3. `POST /api/v1/cohorts/phi-cohort-x/aggregate/count` returns a **noise-injected**
   count and records a `dp_budget` spend in the audit ledger.
4. `audit-cli verify-chain` reports a valid chain.
5. `audit-cli cohort-access --cohort phi-cohort-x --from <start> --to <now>` shows
   only expected researchers and no unexpected failures.
6. A token scoped to a *different* cohort is rejected with 403 on every route.

## Step 6 — Go-live

- Record researcher↔cohort↔data-class mapping in the cohort register.
- Calendar reminder for token re-issue and daily `verify-chain`.
- IRB notification per the approved protocol (any deviation triggers rollback).

## Rollback plan

Same shape as the pilot runbook, with PHI-specific additions:

| Step | Action | How |
|---|---|---|
| R1 | Revoke tokens | `admin revoke <nonce>`; if TM-03 not persisted, rotate the signing key and re-issue (forces all tokens invalid). |
| R2 | Quarantine data | Move affected blobs to `*-quarantine`; mark samples/cohort quarantined. |
| R3 | Freeze cohort | Remove `cohort_membership`; queries scoped to it return no data. |
| R4 | Halt ingest | Network-block upload routes for the cohort's data classes. |
| R5 | Audit & report | `verify-chain` + `cohort-access` export; notify privacy officer + IRB. |
| R6 | Data purge | Delete blobs + rows following the retention/deletion policy; record purge in the ledger. For PHI, deletion must be verifiable (object store + DB row gone). |

**Restore:** re-create cohort + membership from validated source files (never
quarantined data), re-issue scoped tokens, re-run Step 5 acceptance.

## Open items carried from the threat model

- TM-03 (revocation persistence) and TM-06 (audit append durability) should be
  closed before PHI is relied upon for compliance reporting. Until then, the
  mitigations above (short token lifetime, monitored dead-letter) are mandatory.
