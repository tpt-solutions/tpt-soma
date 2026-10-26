# Changelog

All notable changes to `tpt-soma-capability` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## Initial development

### Added

- Graph-scope field on `CapabilityToken`, for organ-system graph queries. (2026-08-08)
- Phase 3 and Phase 4 data-class seeding in `DataClassRegistry`. (2026-08-08)
- Security tests for capability middleware. (2026-08-04)
- Ed25519 signed `CapabilityToken`, HMAC-chained `AttenuatedToken`, `RevocationList`, `DataClassRegistry` and the `tpt-soma-capability` CLI. (2026-08-02)

### Changed

- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
