# tpt-soma-sandbox

Capability-gated host API surface for researcher-submitted compute (WASM sandbox).

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-sandbox` is the seam through which researchers run their own code against platform data. Such code must go through this crate's host API, and the host checks the same capability token used everywhere else before any compute runs. Execution is pluggable: a passthrough backend for tests and local work, and a `wasmtime` backend that runs the code inside a WebAssembly sandbox.

## Features

- **`ResearcherCompute` trait**: the contract a backend implements. `run` takes input bytes and returns output bytes. The rest of the platform sees only this trait, so backends can be swapped.
- **`execute_capability_scoped`**: the gate. It runs an operation only when the token's resource class matches the required class and its action matches the required action. An `admin` token is accepted for any action. Any mismatch returns `SandboxError::InsufficientScope` before the operation starts.
- **`PassthroughCompute`**: a no-op backend that returns its input. Use it for tests and local development. It provides no isolation.
- **`WasmtimeCompute`** (feature `wasmtime-backend`): runs a WebAssembly module with `wasmtime`, loaded from a file (`WasmtimeCompute::from_file`) or built in memory (`WasmtimeCompute::new`). The module follows the convention documented in `src/lib.rs`: it exports `run`, and the host passes the input through the module's memory.

## Features (Cargo)

| Feature | Default | Effect |
|---|---|---|
| `wasmtime-backend` | no | Enables `WasmtimeCompute` and the `wasmtime` dependency |

## Security model

1. A researcher submits a module and a token.
2. The host checks the token's resource class and action against what the operation needs.
3. Only then is the module run, inside the backend.

Isolation comes from the WebAssembly runtime. The gate in step 2 controls what data the call is allowed to ask for. Do not use `PassthroughCompute` for untrusted code.

See [ADR 007](../../docs/adr/007-deferred-items-status.md) for the sandbox plan and status, and [ADR 002](../../docs/adr/002-custom-capability-tokens.md) for the token model.

## Status

The sandbox is not yet wired into [`tpt-soma-api`](../tpt-soma-api/README.md): no route calls `execute_capability_scoped` yet. Isolation depends on the `wasmtime-backend` feature, so do not treat `PassthroughCompute` as a sandbox.

## Crate layout

| Item | Responsibility |
|---|---|
| `ResearcherCompute` | Backend trait |
| `execute_capability_scoped` | Capability gate |
| `PassthroughCompute` | Test and development backend |
| `WasmtimeCompute` | WebAssembly backend (feature `wasmtime-backend`) |
| `SandboxError`, `Result` | Errors: `InsufficientScope`, `Compute` |

## Running the tests

```sh
cargo test -p tpt-soma-sandbox
cargo test -p tpt-soma-sandbox --features wasmtime-backend
```

## Related crates

- [`tpt-soma-capability`](../tpt-soma-capability/README.md): the tokens this crate checks
- [`tpt-soma-api`](../tpt-soma-api/README.md): the intended host for this sandbox (not yet wired)
- [`tpt-soma-audit`](../tpt-soma-audit/README.md): the ledger that would record each run

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
