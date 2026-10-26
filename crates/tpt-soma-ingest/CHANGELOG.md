# Changelog

All notable changes to `tpt-soma-ingest` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Ingest uses the in-house `tpt-soma-flight` stack and the workspace `deny.toml` license policy. (2026-10-25)

## Initial development

### Added

- Binary stream parsing. (2026-08-04)
- Checksum-on-write uploads, with quarantine for files that fail to parse.
- h5ad fixture tests (dense and sparse) and golden-file tests for VCF and h5ad output. (2026-08-03)
- VCF parser (`noodles`) and AnnData/h5ad parser (`hdf5-reader`), with multipart upload endpoints. (2026-08-02)

### Changed

- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
