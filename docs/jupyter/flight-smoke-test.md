# JupyterLab / Flight Smoke Test

Validates the `pyarrow.flight` client path against the tpt-soma Arrow Flight
service (Phase 1 TODO item). Since Flight authentication (TM-02) is enabled, a
capability token is passed as an `authorization` header on every call.

## Prerequisites

- The tpt-soma stack running (`docker compose -f deploy/docker-compose.yml up -d`)
- Python 3.10+ with `pyarrow>=15` (`pip install pyarrow`)
- A capability token issued for the data type you want to query

## Issue a token

```bash
cargo run -p tpt-soma-api --bin tpt-soma-admin -- gen-key
cargo run -p tpt-soma-api --bin tpt-soma-admin -- issue \
  --subject researcher-1 --resource-class genomic_variant \
  --action read --cohort '*' --key dev-keys/signing_key.bin
```

Copy the printed JSON token.

## Option A — Notebook (JupyterLab)

1. Open `scripts/jupyterlab/flight_smoke_test.ipynb`.
2. Set the `TPT_TOKEN` env var (or paste the token in the notebook).
3. Run all cells. Expected: unauthenticated request rejected, then an
   authorized `variants` fetch returning batches.

## Option B — Script

```bash
$env:TPT_FLIGHT_URL = "grpc://localhost:8815"
$env:TPT_TOKEN = '{"subject": ...}'   # the full token JSON

python scripts/jupyterlab/flight_smoke_test.py --data-type variants
# or pass --token-file path/to/token.json
```

Expected output:

```
Connecting to grpc://localhost:8815 ...
OK: unauthenticated request rejected (FlightUnauthenticatedError)
OK: fetched 1 batch(es), N row(s) for 'variants:<sample_id>'
```

## Other data types

- `expression`, `umap` → token resource class `transcriptomic_scrna`
- `clinical_observations` → token resource class `clinical_observation`
- `cgm` → token resource class `cgm_continuous`

A token for the wrong data class is rejected with
`FlightPermissionDeniedError` (permission denied).

## Status

Script and notebook added (2026-08). The test itself is a **manual smoke
test** to run against a live stack; the equivalent automated check is
`crates/tpt-soma-api/tests/e2e_flight.rs` (DB-backed, `#[ignore]`).
