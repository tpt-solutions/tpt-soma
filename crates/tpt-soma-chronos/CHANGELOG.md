# Changelog

All notable changes to `tpt-soma-chronos` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## Initial development

### Added

- Dexcom and FreeStyle Libre CGM parsers for both CSV files and byte streams. (2026-08-04)
- Golden-file tests for CGM parsing, resampling and variability. (2026-08-04)
- CGM and organ-function trajectory storage layers, with `DATA_CLASS_CGM_CONTINUOUS` and `DATA_CLASS_ORGAN_FUNCTION_TRAJECTORY`. (2026-08-03)
- Resampling, gap filling, rolling statistics, glycemic variability and trajectory analysis. (2026-08-02)

### Changed

- Dual MIT OR Apache-2.0 licensing. (2026-08-08)
