# Examples

Walkthroughs for a new contributor or pilot researcher. Work through them in
order. Each step says whether it runs without the stack or needs the full
Compose deployment.

## 1. Build and run the tests (no database)

```sh
cargo build --workspace
cargo test --workspace
```

The database-backed tests are `#[ignore]`d. See [CONTRIBUTING.md](../CONTRIBUTING.md)
for how to run them.

## 2. Create a signing key and a token (no database)

```sh
cargo run -p tpt-soma-capability --bin tpt-soma-capability -- gen-key -o ./dev-keys

cargo run -p tpt-soma-capability --bin tpt-soma-capability -- list-classes

cargo run -p tpt-soma-capability --bin tpt-soma-capability -- issue \
  --subject researcher-1 \
  --resource-class clinical_observation \
  --cohort '*' \
  --action write \
  --key ./dev-keys/signing_key.bin \
  --output token.json
```

`token.json` is the bearer credential. Every API call sends it as
`Authorization: Bearer <contents of token.json>`. Keep it out of version
control.

A token only reaches what its resource class, action and cohort allow. If a
request returns `403`, check the token's scope against the route's entry in
`POLICY` in [crates/tpt-soma-api/src/auth.rs](../crates/tpt-soma-api/src/auth.rs).

## 3. Start the stack (needs the Keystone image)

```sh
cp deploy/.env.example deploy/.env   # set the bootstrap credentials
docker compose -f deploy/docker-compose.yml up -d
curl -s http://localhost:8080/health
```

The Keystone image `ghcr.io/tpt-keystone-db/tpt-keystone` is private. Without
access you cannot run this step, and steps 4 and 5 are unavailable too.

## 4. Upload an organ-function panel (needs the stack)

The template is [docs/templates/organ-function-panel.csv](../docs/templates/organ-function-panel.csv).
Its columns are `patient_id, test_date, test_name, value, unit,
reference_range_low, reference_range_high`. `test_date` is RFC 3339 and
`test_name` must be one of the names the LOINC mapper accepts (for example
`creatinine`, `egfr`, `alt`, `ast`, `hba1c`).

```sh
TOKEN=$(cat token.json)
curl -s -X POST http://localhost:8080/api/v1/ingest/organ-csv \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@docs/templates/organ-function-panel.csv"
```

Each row is validated before it is stored, so one bad row rejects the file.

## 5. Read the trajectory back (needs the stack)

```sh
curl -s "http://localhost:8080/api/v1/clinical-observations/SUBJ-0001/LOINC_CODE/trajectory" \
  -H "Authorization: Bearer $TOKEN"
```

Replace `LOINC_CODE` with the code for the test you uploaded. The read needs a
token with `clinical_observation` and `read` (or `write`) scope.
