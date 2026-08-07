# Load-Test Results Template

Fill one of these per live run of the Phase 1 (TODO line 100) and Phase 2 (TODO
line 145) load-test scaffolding in `scripts/loadtest/`. The scaffolding is
real; a production-scale *run* needs the target hardware, so results are
recorded here manually.

- Date:
- Operator:
- Target stack: (compose | kind/minikube | dedicated)
- Keystone version / sizing (vCPU / memory / disk):
- MinIO sizing:

## Phase 1 — sparse scRNA-seq matrix (TODO line 100)

Seed:
```
python seed_scrna_matrix.py --samples ... --cells ... --genes ... --sparsity ...
```

Endpoint timings (p50 / p95 / p99, ms):

| Endpoint | p50 | p95 | p99 |
|---|---|---|---|
| GET /api/v1/expression/:sample_id |  |  |  |
| GET /api/v1/umap/:sample_id |  |  |  |
| POST /api/v1/join/variant-expression |  |  |  |
| POST /api/v1/cohorts/:cohort_id/aggregate/count |  |  |  |

- Error rate at sustained load:
- `audit-cli verify-chain` valid immediately after run: (yes / no)
- DP budget exhaustion behavior under concurrency:

## Phase 2 — Chronos longitudinal CGM (TODO line 145)

Seed:
```
python seed_chronos.py --patients ... --years ... --points-per-day ...
```

Endpoint timings (p50 / p95 / p99, ms):

| Endpoint | p50 | p95 | p99 |
|---|---|---|---|
| GET /api/v1/cgm/:subject_id |  |  |  |
| GET /api/v1/cgm/:subject_id/variability |  |  |  |
| POST /api/v1/cohorts/:cohort_id/aggregate/count |  |  |  |

- Error rate at sustained load:
- `audit-cli verify-chain` valid immediately after run: (yes / no)

## Notes / follow-ups
