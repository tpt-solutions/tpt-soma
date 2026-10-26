# Changelog

All notable changes to `tpt-soma-api` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Clinical trial routes (`/api/v1/trials`, screening, adverse events and AE summary) behind the capability and audit choke point. (2026-10-26)
- Device-state write path over Arrow Flight `DoPut`, audited in the ledger. (2026-10-25)
- Systemic query endpoint (`/api/v1/systemic/query`) with the `simulation_output` policy. (2026-10-25)
- In-house metrics module that renders Prometheus text format at `/metrics`. (2026-10-25)

### Changed

- Flight service moved to the in-house `tpt-soma-flight` stack in place of `arrow-flight`. (2026-10-25)

## Initial development

### Added

- Simulation endpoints, differential privacy on simulation aggregates, and Chronos/Plexus mirroring of simulation outputs. (2026-08-08)
- Capability middleware security tests and audit-chain tampering tests. (2026-08-04)
- Phase 2 Chronos and Organon storage integration. (2026-08-03)
- Phase 1 REST and Flight server, capability middleware and audit integration. (2026-08-02)

### Changed

- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
