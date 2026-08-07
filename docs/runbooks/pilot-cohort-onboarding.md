# Pilot Cohort Onboarding Runbook + Rollback Plan

Purpose: onboard a small pilot cohort end-to-end — from token issuance through
data ingestion to verified query access — with a defined rollback path.
Gate: the threat-model sign-off and self-pen-test checklist must be complete
before this runbook is executed with any patient-linked data.

## Preconditions

- [ ] Local/Compose stack healthy: `keystone`, `minio`, `api`, `frontend`, `flight`.
- [ ] `docs/security/self-pentest-checklist.md` executed and recorded.
- [ ] Threat model sign-off completed.
- [ ] Pilot cohort definition approved (which samples, which data classes, which researchers).

## Step 1 — Environment & secrets

1. Create `.env` from `deploy/.env.example`.
2. Set strong values for `POSTGRES_PASSWORD`, `MINIO_ROOT_PASSWORD`,
   `TPT_AUTH_BOOTSTRAP_PASSWORD`.
3. Generate the capability root signing key (do **not** commit it):
   ```
   cargo run -p tpt-soma-api --bin tpt-soma-admin -- gen-key
   # writes dev-keys/signing_key.bin + verifying_key.bin
   ```
4. Mount the signing key at `CAPABILITY_ROOT_KEY_PATH` in the API service
   (Compose: `capability-secrets` volume; Helm: secret, see deployment docs).

## Step 2 — Create the cohort

Insert the cohort + membership (SQL against Keystone, e.g. via psql):

```sql
INSERT INTO cohorts (id, name, description, is_public)
VALUES ('cohort-a', 'Pilot Cohort A', 'initial pilot', false);
```

Cohort membership is linked by `sample_id`/`subject_id` in
`cohort_membership`. For the pilot keep the cohort list small and explicit.

## Step 3 — Issue researcher tokens

For each approved researcher:

```
   cargo run -p tpt-soma-api --bin tpt-soma-admin -- issue \
   --subject researcher-1 \
   --resource-class genomic_variant \
   --action read \
   --cohort cohort-a \
   --expiry 3600 \
   --key dev-keys/signing_key.bin
```

Distribute the printed JSON token out-of-band. Tokens honor the `--expiry`
flag (seconds from now; threat model TM-05 is fixed); refresh by re-issuing.

## Step 4 — Ingest reference data

For Phase 1 (public/open-consent data only):

```
# VCF
curl -H "Authorization: Bearer $TOKEN" \
  -F "file=@sample.vcf" \
  http://localhost:8080/api/v1/ingest/vcf

# AnnData / h5ad
curl -H "Authorization: Bearer $TOKEN" \
  -F "file=@sample.h5ad" \
  http://localhost:8080/api/v1/ingest/h5ad
```

- Verify objects land in MinIO `raw-omics` with matching checksums.
- Verify parsed rows land in `variants`/`sample_variants`/`scrna_expression`.
- Confirm no files went to the quarantine bucket (all parse successes).

## Step 5 — Harmonization review

Run `review-cli` (in `tpt-soma-harmonize`) to list unmapped identifiers:

```
cargo run -p tpt-soma-harmonize --bin review-cli -- list
cargo run -p tpt-soma-harmonize --bin review-cli -- resolve <id> <target>
```

Resolve any unmapped rsIDs/gene symbols before relying on join queries.

## Step 6 — Verification (acceptance)

1. `GET /api/v1/variants/<sample_id>` returns expected variants.
2. `GET /api/v1/expression/<sample_id>` returns expected cells/genes.
3. `POST /api/v1/join/variant-expression` returns rows for the pilot sample.
4. `POST /api/v1/cohorts/cohort-a/aggregate/count` returns a noisy count and an
   audit `dp_budget` spend event exists.
5. `audit-cli verify-chain` reports a valid chain.
6. `audit-cli cohort-access --cohort cohort-a --from <start> --to <now>` shows
   the expected success events and no unexpected failures.

## Step 7 — Go-live announcement

- Record researcher-to-cohort mapping in the pilot register.
- Announce to researchers with token-usage and data-handling instructions.
- Set a calendar reminder for token re-issue and audit-chain verification.

## Rollback plan

**Triggers for rollback:** data integrity issue, unexpected access pattern,
audit chain tampering, researcher token compromise, DP budget anomaly.

| Step | Action | How |
|---|---|---|
| R1 | Revoke tokens | `admin revoke <nonce>` for affected tokens (persisted-revocation path once TM-03 is closed; otherwise restart + re-issue with new keys). |
| R2 | Quarantine data | Move affected blobs to `*-quarantine` bucket; mark affected samples/cohort as quarantined in DB. |
| R3 | Freeze cohort | Remove `cohort_membership` rows; queries scoped to it return no data. |
| R4 | Halt ingest | Disable ingest routes (or network-block uploads) for the cohort's data classes. |
| R5 | Audit & report | Run `verify-chain`, export `cohort-access` report, notify privacy officer. |
| R6 | Data purge (if required) | Delete blobs + rows for the affected cohort following the retention/deletion policy; record purge in the ledger. |

**Restore after rollback:** re-create the cohort and membership, re-issue
tokens, re-ingest from validated source files (never from quarantined data).

## Phase 2 extension (real PHI)

For patient-linked onboarding see `docs/runbooks/real-phi-onboarding.md`. The
differences are: IRB approval required first, FHIR/CGM ingestion paths,
imaging blobs, and the additional data classes (`clinical_observation`,
`cgm_continuous`, `organ_imaging`) in token scope.
