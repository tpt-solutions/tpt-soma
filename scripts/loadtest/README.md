# Load Test Scaffolding

Scaffolding for the two load-test TODO items that require a running Keystone +
MinIO instance (not exercised by CI):

- **TODO line 100** — Keystone load test at realistic scale for the sparse
  scRNA-seq matrix (`scrna_expression(sample_id, cell_id, gene_id, count)`,
  ~thousands of cells × thousands of genes × N samples).
- **TODO line 145** — Keystone Chronos load test at realistic longitudinal scale
  (`cgm_readings`, ~10^5 points/patient/year × N patients × years).

These scripts are **scaffolding**: they seed realistic-scale data and exercise
the query paths, but running them against a production-scale instance (and
capturing p95/p99 latency, error rate, chain-integrity under load) is a manual
activity that needs the target hardware. Wire them into a scheduled job or run
them ad hoc against a dedicated load-test Keystone.

## Layout

- `seed_scrna_matrix.py` — populates a sparse scRNA matrix at configurable
  (samples × cells × genes) scale.
- `seed_chronos.py` — populates CGM time series at configurable
  (patients × points/year × years) scale with realistic gaps/trend arrows.
- `api_load.js` — [k6](https://k6.io) script hitting the API query + aggregate
  endpoints with a capability token (reads schema, variants, expression, umap,
  clinical, cgm, and the DP aggregate export).

## Prerequisites

```
pip install psycopg[binary]   # seed scripts
# k6: https://k6.io/docs/get-started/installation/
export TEST_DATABASE_URL=postgres://admin:pass@localhost:5432/tpt_soma
export TPT_TOKEN='{...capability token JSON...}'   # read/export scoped
```

## Seed the sparse scRNA matrix

```bash
python seed_scrna_matrix.py \
  --samples 50 --cells 5000 --genes 20000 --sparsity 0.9
# default scale above is ~50 samples × 5000 cells × 20000 genes.
# Drop --cells/--genes for a quick smoke seed; raise them for a full-scale run.
```

This bulk-inserts into `scrna_expression` via COPY and refreshes the relevant
indexes. Re-run with `--truncate` to reset.

## Seed the Chronos CGM series

```bash
python seed_chronos.py \
  --patients 100 --years 2 --points-per-day 288
# 288 readings/day ≈ 5-min Dexcom interval; 100 patients × 2 yrs ≈ 2.1e7 rows.
```

## Run the API load test

```bash
k6 run -e TPT_API_URL=http://localhost:8080 -e TPT_TOKEN="$TPT_TOKEN" api_load.js
```

`api_load.js` ramps VUs up and holds, then ramps down, reporting HTTP timings
per endpoint. The DP `aggregate/count` calls assert a 200 and a numeric payload;
if the epsilon budget is exhausted mid-run the test surfaces the 4xx so you can
confirm the budget guard holds under concurrency.

## What "done" looks like

Record results in `docs/testing/loadtest-results-<date>.md`:

- p50/p95/p99 latency for each endpoint at target scale,
- error rate at sustained load,
- audit `verify-chain` still valid immediately after the run,
- DP budget exhaustion behavior under concurrency.
