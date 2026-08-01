# ADR-005: Schema Evolution Policy

## Status

Accepted

## Context

The project uses multiple serialization formats across the same data contract:

- **Arrow schemas** — for analytical data exchange (Flight RPC, Parquet files in object store).
- **Protobuf** — for high-speed service-to-service serialization.
- **SQL migrations** — for Keystone's relational tables.

A naive schema change (e.g., removing a field, changing a type) can silently break downstream consumers: a Jupyter notebook expecting a field that a new Arrow schema no longer provides, or a Protobuf consumer that can't deserialize a migrated message.

## Decision

Adopt an **additive-only within major version** evolution policy:

- **Arrow**: New fields may be added. Removing or renaming a field requires bumping the schema major version. Consumers must handle missing fields gracefully (Arrow's nullability + default values).
- **Protobuf**: Fields are never reused or repurposed; retired fields are reserved by number. New major versions are separate `.proto` files (e.g., `v2/sample.proto`).
- **SQL**: Migrations are plain SQL files in `tpt-soma-core/migrations/`, versioned sequentially. Columns are never dropped in-place; deprecation marks the column as unused, and a separate migration removes it in the next major release.

Compatibility is enforced in CI:

- Arrow schema compatibility check (addeditive rules).
- Protobuf lint + binary compatibility test.
- SQL migration dry-run against a fresh database.

## Consequences

- **Positive**: Consumers can upgrade independently without breaking the data pipeline.
- **Positive**: Clear policy removes ad-hoc decisions about whether a change is "breaking."
- **Trade-off**: Schema debt accumulates if old fields are never cleaned up. Mitigated by a quarterly review of deprecated fields and a migration to remove them in the next major version.
