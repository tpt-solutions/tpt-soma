# ADR-001: Storage Consolidation on tpt-keystone-db

## Status

Accepted

## Context

The original design planned a polyglot persistence layer with three separate storage technologies: DuckDB for embedded analytics, Postgres for relational data, and a dedicated graph database (ArangoDB or Neo4j) for the Ontological Soma Graph topology. This was driven by the assumption that each data shape (tabular, relational, graph) required a specialized engine.

During early architecture review, the sibling project `tpt-keystone-db` became available. It is a single Postgres-wire-compatible engine that already includes:

- **Plexus** — graph extension for OSG topology and edge traversal
- **Chronos** — time-series extension for longitudinal trajectories
- **Canopy** — JSON/document extension for raw FHIR payloads

Keystone speaks standard Postgres wire protocol on port 5432 and also exposes an HTTP/JSON bridge on 5435. It does not require binding to a custom SDK or version.

## Decision

`tpt-soma` will consolidate **all** storage on `tpt-keystone-db` from Phase 0 onward. No DuckDB, no standalone Postgres, and no separate graph database will be introduced.

The connection path is standard Postgres wire via `sqlx`/`tokio-postgres`, **not** Keystone's own SDK. This keeps `tpt-soma`'s domain crates decoupled from Keystone's release cadence and preserves the option to migrate to another Postgres-compatible store in the future without rewriting domain logic.

## Consequences

- **Positive**: Single operational substrate, reduced ops overhead, consistent ACID guarantees across all data shapes.
- **Positive**: Eliminates the Phase-2 ArangoDB-vs-Neo4j bake-off entirely.
- **Trade-off**: `tpt-soma`'s velocity is coupled to Keystone's maturity. Keystone is described in its own README as "not a production-hardened platform" for its first-party use cases. This is mitigated by the Postgres-wire abstraction layer (a future migration off Keystone is possible without touching domain crates).
- **Trade-off**: Graph query performance may lag a native graph DB for very deep traversals. The initial OSG topology in Phase 1 is small enough that this is not a concern; we will revisit if traversal latency becomes measurable.
