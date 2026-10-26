# Changelog

All notable changes to `tpt-soma-organon` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## Initial development

### Added

- Organ-system graph with paths, neighbors and cascade simulation, persisted through `to_file` and `from_file`. (2026-08-03)
- Reference-range configuration (`config/reference_ranges.json`). (2026-08-04)
- Organon storage layer and Phase 2 migration, covering clinical observations and organ imaging. (2026-08-03)
- FHIR and organ-function CSV ingestion, DICOM metadata parsing, LOINC constants and clinical calculators, including eGFR (CKD-EPI 2021). (2026-08-02)

### Changed

- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
