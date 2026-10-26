# Changelog

All notable changes to `tpt-soma-core` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Migration 10: clinical trial results (`trial_enrollments`, `trial_adverse_events`). (2026-10-26)
- Migration 9: device-state observations, written through Arrow Flight `DoPut`. (2026-10-25)

## Initial development

### Added

- Cross-domain differential-privacy aggregate exports. (2026-08-08)
- Federated result envelopes with HMAC authentication and ledger consistency proofs. (2026-08-08)
- Security tests for capability middleware and audit tampering. (2026-08-04)
- Connection pool, query helpers, differential privacy, and migrations 1 to 8 (audit ledger, Phase 1 schema, scRNA UMAP, Plexus graph, Phase 2 Chronos and Organon, Phase 3 simulacrum, Phase 4 OSG, simulation mirroring).

### Changed

- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
