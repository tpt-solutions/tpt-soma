# Changelog

All notable changes to `tpt-soma-clinica` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Persistence of trial protocols, screening decisions and adverse events, backed by migration 10 in `tpt-soma-core`. (2026-10-26)
- Adverse-event summary (`summarize_adverse_events`) and eligibility screening (`evaluate_eligibility`, `screen_and_recruit`). (2026-10-25)
- In progress: Welch t-test, Pearson and point-biserial statistics, cohort query builder, and biomarker association pipeline.

## Initial development

### Added

- Clinical analytics crate skeleton. (2026-08-08)

### Changed

- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
