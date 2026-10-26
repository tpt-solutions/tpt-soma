# Changelog

All notable changes to `tpt-soma-harmonize` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## Initial development

### Added

- SNOMED CT mappings and the Phase 1 and Phase 2 seed tables (`seed_phase1_phase2`). (2026-08-04)
- `MappingTable`, `ReviewQueue`, deterministic resolution, and CSV import and export. (2026-08-02)
- `review-cli` binary for working the review queue. (2026-08-02)

### Changed

- CSV I/O helpers extracted into `io` with round-trip tests. (2026-08-03)
- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
