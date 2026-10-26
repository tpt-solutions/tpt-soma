# Changelog

All notable changes to `tpt-soma-genomica` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## Initial development

### Added

- Variant harmonization pipeline with ordered steps and optional database backing. (2026-08-02)
- `Harmonizer` with rsID and gene-symbol mappings, and `VariantAnnotationStore`. (2026-08-02)

### Changed

- Harmonization CSV helpers moved to `tpt-soma-harmonize`, with added coverage tests. (2026-08-03)
- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
