# Changelog

All notable changes to `tpt-soma-sandbox` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `WasmtimeCompute` backend behind the `wasmtime-backend` feature, loading modules from a file or from memory. (2026-08-08)

## Initial development

### Added

- `ResearcherCompute` trait, `execute_capability_scoped` capability gate, and `PassthroughCompute` for tests and local development. (2026-08-08)

### Changed

- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
