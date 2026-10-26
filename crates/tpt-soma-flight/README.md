# tpt-soma-flight

In-house columnar batches, Arrow-compatible IPC, and Flight gRPC (no Apache-2.0-only dependencies).

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-flight` implements the subset of the Apache Arrow columnar format and the Arrow Flight protocol that the platform needs. It exists so that the platform's dependency chain stays usable under plain MIT. The stack is written from the public Arrow and Flight specifications, and it interoperates with `pyarrow.flight` and other standard clients.

See [ADR 008](../../docs/adr/008-in-house-flight-stack.md) for why this crate replaces the `arrow`, `arrow-flight` and `prost` crates.

## Features

- **Columnar model** (`columnar`): `Schema`, `Field`, `RecordBatch` and `Column` for five value types: `Utf8`, `Int32`, `Int64`, `Float64` and `Boolean`. Columns are nullable. Construction is checked: `RecordBatch::try_new` rejects mismatched lengths or types.
- **Arrow IPC** (`ipc`): reads and writes Arrow IPC messages with metadata version V5. `encapsulate` and `strip_encapsulation` handle the continuation framing. The reader is written defensively, because `DoPut` accepts untrusted bytes.
- **Protobuf encoding** (`pb`): a small protobuf encoder and decoder (`PbEncoder`, `PbDecoder`, `PbCodec`) for the Flight messages (`FlightData`, `FlightInfo`, `FlightDescriptor`, `FlightEndpoint`, `PutResult`, `HandshakeRequest` and `HandshakeResponse`).
- **Flight service** (`service`, feature `service`): a gRPC server on `tonic` implementing `Handshake`, `GetFlightInfo`, `DoGet` and `DoPut`. `FlightClient` is a small client for the same calls. Other RPCs return `UNIMPLEMENTED`, and `GetSchema` is not implemented.
- **Data helpers** (`data`): `batch_flight_data`, `flight_data_to_batches` and friends convert between batches and Flight messages.

Unsupported input is rejected with an error. It is never decoded into a different type.

## Features (Cargo)

| Feature | Default | Effect |
|---|---|---|
| `service` | yes | Enables the gRPC server and client, and pulls in `tonic` |

Build with `--no-default-features` to get only the columnar, IPC and protobuf parts. That build has no `tonic` and no `sync_wrapper`, so other projects that need only the wire format can use it without a gRPC stack.

## Limits

- Types: `Utf8`, `Int32`, `Int64`, `Float64`, `Boolean`, all nullable. No dictionary, nested, decimal or temporal types.
- No compression.
- Only the Flight RPCs listed above are routed.

Adding a type means changing `columnar.rs`, `ipc.rs` and the interop script together. See the consequences section of ADR 008.

## Interoperability check

The example `interop_server` serves the Flight calls for testing from Python:

```sh
cargo run -p tpt-soma-flight --example interop_server
```

Then run `scripts/flight_interop_check.py` (it uses `pyarrow.flight`). The script checks `GetFlightInfo`, `DoGet` with full validation, `DoPut`, and an unimplemented call. Run it after any change to `ipc.rs`, `pb.rs` or `service.rs`.

## Crate layout

| Module | Responsibility |
|---|---|
| `columnar` | Schema, field, column and record batch types |
| `ipc` | Arrow IPC read and write |
| `pb` | Protobuf codec for Flight messages |
| `fb` | Internal FlatBuffers helpers for IPC metadata (not public) |
| `data` | Conversions between batches and Flight data |
| `service` | gRPC server and client (feature `service`) |
| `error` | `Error`, `Result` |

## Running the tests

```sh
cargo test -p tpt-soma-flight
cargo test -p tpt-soma-flight --no-default-features
```

Run both, because the no-default build is what other projects depend on.

## Related crates

- [`tpt-soma-api`](../tpt-soma-api/README.md): serves the Flight service to clients

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
