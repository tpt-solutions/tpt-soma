# Unit Test Coverage Targets

Formal coverage targets for the ingest/harmonize/genomica/cytos crates (Phase 1
TODO item). Targets are stated as minimum **line coverage** of the crate's
non-test, non-generated code, measured by `cargo-llvm-cov` on Linux CI.

## Targets per crate

| Crate | Minimum line coverage | Notes |
|---|---|---|
| `tpt-soma-capability` | 85% | Security-critical: token sign/verify, attenuation, revocation, signing backends |
| `tpt-soma-audit` | 80% | Security-critical: ledger hashing, chain verification |
| `tpt-soma-core` | 70% | DP module 90%+; connection/migrations/query helpers cover what's feasible without DB |
| `tpt-soma-ingest` | 70% | VCF parser, h5ad parsing (fixture-backed), quarantine/upload logic |
| `tpt-soma-harmonize` | 75% | Mapping tables, review queue, CSV I/O round-trips |
| `tpt-soma-genomica` | 60% | Annotation/harmonizer, pipeline steps |
| `tpt-soma-cytos` | 60% | Scanpy script generation, cluster-map parsing, storage helpers |
| `tpt-soma-organon` | 65% | FHIR/CSV ingestion, calculators, reference ranges, imaging metadata |
| `tpt-soma-chronos` | 65% | CGM parsers, resampling, variability, trajectory |
| `tpt-soma-api` | 60% | Middleware/auth paths (in-memory tests), flight encoding |

## Measurement

- Tool: `cargo-llvm-cov` (`cargo install cargo-llvm-cov`).
- Command:
  ```
  cargo llvm-cov --workspace --exclude-explicitly-unused \
    --ignore-filename-regex 'tests/|/tests/' --summary-only
  ```
- CI: `.github/workflows/ci.yml` `coverage` job enforces per-crate thresholds
  via `cargo llvm-cov` and fails the build when a target is missed.

## Coverage-vs-meaning rules

- Coverage is a floor, not a goal. New security-critical code in
  `capability`/`audit`/`dp` must keep the 80–90% bars.
- DB-backed paths are covered by `#[ignore]` integration tests; line coverage
  for those paths is **not** required to hit the per-crate targets (CI cannot
  run a database). Track them separately in the load/DB test suites.
- A crate that drops below its target must add tests in the same PR that
  restores it (no "coverage debt" merges).

## Current status (2026-08)

Measured informally: harmonize CSV I/O + mapping + review tests, h5ad fixture
tests, and golden-file suites exist and are substantial. Formal thresholds are
now defined above and enforced in CI; the first full `cargo llvm-cov` run
should be recorded in `docs/testing/coverage-results-<date>.md` alongside any
shortfalls and remediation.
