# Changelog

All notable changes to `tpt-soma-audit` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Integrity alerting: `AlertSink`, `TracingAlertSink`, `IntegrityAlert`, and `check_and_alert` to report chain failures. (2026-10-26)

## Initial development

### Added

- Security tests for audit-chain tampering. (2026-08-04)
- Compliance report generator and `tpt-soma-audit` CLI. (2026-08-02)
- Append-only, hash-chained `AuditLedger` with chain verification and the integrity job. (2026-08-02)

### Changed

- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
