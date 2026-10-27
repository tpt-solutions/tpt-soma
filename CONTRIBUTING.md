# Contributing

Contributions are handled through issues only. Please open an issue to report a
bug, request a feature or propose a change. Do not open pull requests; they will
be closed without review.

The sections below are for maintainers working on the codebase.

## Checks

Run the same checks CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

The frontend lives in `frontend/`:

```sh
cd frontend
npm ci
npx tsc -b
npx eslint .
npx vitest run
```

## Database-backed tests

Many tests need PostgreSQL and are `#[ignore]`d. They delete rows, so point them
at a disposable database. The variable has no default on purpose:

```sh
export TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/tpt_soma_test
cargo test -p tpt-soma-audit -- --include-ignored --test-threads=1
```

- Run with `--test-threads=1`. These tests share a table.
- Set `TEST_SKIP_MIGRATIONS=1` to run the audit tests on plain PostgreSQL with
  only the audit schema applied (see the `audit-database` job in
  `.github/workflows/ci.yml`).
- The full suite needs the Keystone image, because the migrations use its
  `plexus`, `chronos` and `canopy` extensions.

## Rules that maintainer review checks

- **Every route needs a `POLICY` entry** in `crates/tpt-soma-api/src/auth.rs`.
  Unlisted routes are denied, and the `every_registered_route_has_a_policy_entry`
  test fails until the entry exists.
- **Register data classes through `DataClassRegistry::seed_all()`.** Do not seed
  a subset, because entry points would then disagree.
- **Anything that releases a result must record it first.** Differential-privacy
  exports call `release_dp_count`. Responses are released only after their audit
  row is written.
- **No real patient data in tests, fixtures or examples.** Real data is gated by
  [the real-PHI onboarding gate](docs/runbooks/real-phi-onboarding-gate.md).
- Architecture decisions go in an ADR under `docs/adr/`.

## Changelogs

Each crate keeps a `CHANGELOG.md` in the Keep a Changelog format. Add an entry
under "Unreleased" for any user-visible change.
