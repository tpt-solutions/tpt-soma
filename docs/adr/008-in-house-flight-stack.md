# ADR-008: In-house columnar / Flight stack, no Apache-2.0-only dependencies

## Status

Accepted

## Context

The workspace is released as `MIT OR Apache-2.0`. Consumers who need a pure-MIT dependency chain cannot use a build that includes crates licensed Apache-2.0 only. The Arrow stack (`arrow*`, `arrow-flight`), `flatbuffers`, `prost`, `prometheus` and (through the unused `anndata` dependency) `nalgebra`, `polars` and `openssl` were all in that set.

## Decision

- **Removed:** `anndata` (unused; `h5ad.rs` parses with `hdf5-reader`), which also removed `nalgebra`, `polars` and the `openssl` path. `reqwest` now builds with `default-features = false` so `native-tls` stays out.
- **Replaced `prometheus`** with `tpt-soma-api/src/metrics.rs` (counters, Prometheus text format).
- **Replaced `arrow` / `arrow-flight` / `prost`** with the in-house `tpt-soma-flight` crate: a five-type columnar model, Arrow IPC (metadata V5) read/write, protobuf encoding of the Flight messages, and the Flight gRPC service and a small client on `tonic`'s MIT transport. It was written from the public Arrow columnar and Flight specifications.
- **Enforced** by `deny.toml`: `Apache-2.0` is not in the license allow list, so a new Apache-only crate fails `cargo deny check licenses` unless added to `exceptions`. The only exceptions are `sync_wrapper` (tower/axum/tonic/reqwest) and `ring` (rustls).

## Scope and limits

Supported types: Utf8, Int32, Int64, Float64, Boolean, nullable. No dictionaries, compression, nested types, or other Arrow types; unsupported input is rejected with an error, never mis-decoded. Only `Handshake`, `GetFlightInfo`, `GetSchema` (unimplemented), `DoGet` and `DoPut` are routed; other calls return gRPC `UNIMPLEMENTED`, as before.

## Verification

`scripts/flight_interop_check.py` runs `pyarrow.flight` against `cargo run -p tpt-soma-flight --example interop_server`: `GetFlightInfo`, `DoGet` (with `validate(full=True)`), `DoPut` and an unimplemented call. Rerun it after any change to `ipc.rs`, `pb.rs` or `service.rs`. The IPC reader is exercised with truncated and bit-flipped input because `DoPut` accepts untrusted bytes.

## Consequences

- We own the wire-format code and must keep it compatible with Arrow clients as they evolve. Adding a type means touching `columnar.rs`, `ipc.rs` and the interop script.
- `GetFlightInfo` now echoes the request descriptor; Arrow C++ (pyarrow) rejects a `FlightInfo` without one.
- `tonic` is behind the `service` feature, which is on by default. Build with `--no-default-features` to use only the columnar, IPC and protobuf parts. That build has no tonic and no `sync_wrapper`. Other tpt repos can use it without a gRPC stack.
