# Real-PHI Pilot Onboarding — Pre-Flight Gate

TODO line 135 (first patient-linked cohort onboarded end-to-end through the
Phase 0 capability/audit/DP stack) is **blocked on IRB approval + real patient
data**. This checklist is the sign-off gate that must be fully green before the
first real-PHI cohort is onboarded against `tpt-soma`. The procedural runbook
is `docs/runbooks/real-phi-onboarding.md`.

Every item below is a hard gate. Do not onboard real PHI until all are checked.

## 1. Governance & approval
- [ ] IRB / ethics approval granted for this specific cohort + data use.
- [ ] Data-use agreement (DUA) executed with the contributing site.
- [ ] Lawful basis & consent model documented (GDPR Art. 6/9 or HIPAA equivalent).
- [ ] Retention + deletion schedule agreed and encoded in `tpt-soma-core` policy.

## 2. Security sign-off (open threat-model findings)
The threat model (`docs/security/threat-model.md`) currently carries open
findings that must be resolved or formally accepted by the security owner
before real PHI flows:

- [ ] TM-03 — resolved or risk-accepted with documented compensating control.
- [ ] TM-06 — resolved or risk-accepted with documented compensating control.
- [ ] TM-09 — resolved or risk-accepted with documented compensating control.
- [ ] Self pentest checklist (`docs/security/self-pentest-checklist.md`) re-run
      against the exact deployment that will hold PHI.

## 3. Capability / audit / DP readiness
- [ ] Bootstrap credentials rotated off defaults; `TPT_AUTH_BOOTSTRAP_*` removed
      from any reachable environment.
- [ ] Root signing key for `tpt-soma-capability` under KMS/HSM (not local
      keyfile) in non-dev environments (`signing.rs::KmsSigningBackend`).
- [ ] Revocation list reachable + monitored; revocation path exercised in a
      staging run.
- [ ] Differential-privacy epsilon budgets configured per cohort; budget
      exhaustion verified in staging (see `crates/tpt-soma-core/tests/dp_tests.rs`).
- [ ] `audit-cli verify-chain` wired into a scheduled job with alerting on
      mismatch (currently a printed diagnostic only — see `integrity.rs`).

## 4. Storage & isolation
- [ ] Keystone + MinIO provisioned on isolated, access-controlled infra
      (not the shared dev instance).
- [ ] MinIO bucket layout + checksum-on-write verified; quarantine bucket
      exercised with a malformed real-world file.
- [ ] Backup + restore drill completed for Keystone + object store.

## 5. Pilot runbook dry-run (public/open-consent data)
- [ ] `docs/runbooks/pilot-cohort-onboarding.md` executed end-to-end against
      public reference data with zero deviations.
- [ ] Rollback plan (`docs/runbooks/pilot-cohort-onboarding.md`) rehearsed.

## Sign-off

| Role | Name | Date |
|---|---|---|
| Researcher / PI |  |  |
| Security owner |  |  |
| Data protection officer |  |  |
