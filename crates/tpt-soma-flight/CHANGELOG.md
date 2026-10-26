# Changelog

All notable changes to `tpt-soma-flight` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `tonic` is now optional behind the `service` feature, which is on by default. Build with `--no-default-features` for the columnar, IPC and protobuf parts only. (2026-10-26)

## Initial development

### Added

- In-house Arrow-compatible stack replacing `arrow`, `arrow-flight` and `prost`: columnar batches for five types, Arrow IPC (metadata V5), protobuf encoding of Flight messages, and the Flight gRPC service and client. The stack is described in [ADR 008](../../docs/adr/008-in-house-flight-stack.md). (2026-10-25)
- `interop_server` example for checking against `pyarrow.flight`. (2026-10-25)
