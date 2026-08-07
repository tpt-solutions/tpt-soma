//! tpt-soma-sandbox: host API surface for researcher-submitted compute
//! (Phase 4).
//!
//! This crate defines the *host API surface* that researcher-submitted compute
//! must go through, gated by the same capability tokens used everywhere else in
//! `tpt-soma`. The [`ResearcherCompute`] trait is the seam where an execution
//! backend plugs in; the default [`PassthroughCompute`] backend is a no-op used
//! for tests and local development, while the `wasmtime-backend` feature
//! provides real WASM isolation reusing the `wasmtime` runtime (the plan in
//! `docs/adr/007-deferred-items-status.md` §2.5 was to bind to Keystone's
//! `wasmtime` UDF sandbox — this self-contained backend is the equivalent
//! in-repo seam, capability-gated exactly like the host API expects).

use thiserror::Error;
use tpt_soma_capability::token::CapabilityToken;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("insufficient scope: {0}")]
    InsufficientScope(String),
    #[error("compute error: {0}")]
    Compute(String),
}

pub type Result<T> = std::result::Result<T, SandboxError>;

/// A unit of researcher-submitted compute. The concrete implementation may
/// run WASM via the `wasmtime` backend; the trait keeps the host API
/// capability-scoped regardless of backend.
pub trait ResearcherCompute {
    fn run(&self, input: &[u8]) -> Result<Vec<u8>>;
}

/// Execute `op` only if `token` authorizes `required_class` with
/// `required_action` (`read`/`export`; `admin` is also permitted).
pub fn execute_capability_scoped(
    token: &CapabilityToken,
    required_class: &str,
    required_action: &str,
    op: &dyn ResearcherCompute,
    input: &[u8],
) -> Result<Vec<u8>> {
    if token.resource_class != required_class {
        return Err(SandboxError::InsufficientScope(format!(
            "token resource class '{}' != required '{}'",
            token.resource_class, required_class
        )));
    }
    if token.action != required_action && token.action != "admin" {
        return Err(SandboxError::InsufficientScope(format!(
            "token action '{}' != required '{}'",
            token.action, required_action
        )));
    }
    op.run(input)
}

/// Passthrough backend used for tests and local development.
pub struct PassthroughCompute;

impl ResearcherCompute for PassthroughCompute {
    fn run(&self, input: &[u8]) -> Result<Vec<u8>> {
        Ok(input.to_vec())
    }
}

/// Real WASM execution backend (feature `wasmtime-backend`).
///
/// Guest modules must follow a minimal, host-agnostic ABI:
/// - export linear `memory`;
/// - export `fn run(input_len: i32) -> i32`: the host writes the request
///   payload into guest memory at [`GUEST_INPUT_PTR`], then calls `run` with
///   the payload length; the guest writes its result into guest memory at
///   [`GUEST_OUTPUT_PTR`] and returns the result length.
///
/// The host reads the result back out of guest memory after `run` returns, so
/// the guest keeps full control of its own allocator and only needs to honor
/// the two fixed memory offsets. This keeps the contract small enough to
/// implement from any guest toolchain (Rust `cdylib` -> `wasm32-unknown-unknown`,
/// C, TinyGo, handwriting WAT, …).
#[cfg(feature = "wasmtime-backend")]
mod wasmtime_backend {
    use super::{Result, SandboxError};
    use wasmtime::{Engine, Func, Instance, Module, Store, Val};

    const GUEST_INPUT_PTR: usize = 1024;
    const GUEST_OUTPUT_PTR: usize = 2048;

    /// A compiled WASM module plus the engine it was compiled against.
    ///
    /// Cheap to construct once and reuse across many [`ResearcherCompute`]
    /// calls; each call gets a fresh [`Store`] so guest state is isolated per
    /// invocation.
    pub struct WasmtimeCompute {
        engine: Engine,
        module: Module,
    }

    impl WasmtimeCompute {
        /// Compile `wasm` (WASM binary or WAT text) into an executable backend.
        pub fn new(wasm: &[u8]) -> Result<Self> {
            let engine = Engine::default();
            let module = Module::new(&engine, wasm)
                .map_err(|e| SandboxError::Compute(format!("failed to compile guest: {e}")))?;
            Ok(Self { engine, module })
        }

        /// Compile a guest WASM module loaded from `path`.
        pub fn from_file(path: &std::path::Path) -> Result<Self> {
            let bytes = std::fs::read(path)
                .map_err(|e| SandboxError::Compute(format!("failed to read guest: {e}")))?;
            Self::new(&bytes)
        }
    }

    impl super::ResearcherCompute for WasmtimeCompute {
        fn run(&self, input: &[u8]) -> Result<Vec<u8>> {
            let mut store = Store::new(&self.engine, ());
            let instance = Instance::new(&mut store, &self.module, &[])
                .map_err(|e| SandboxError::Compute(format!("failed to instantiate guest: {e}")))?;

            let memory = instance
                .get_memory(&mut store, "memory")
                .ok_or_else(|| SandboxError::Compute("guest missing exported 'memory'".into()))?;

            if GUEST_INPUT_PTR + input.len() > memory.data_size(&store) {
                return Err(SandboxError::Compute(
                    "guest input buffer overflow: input larger than guest memory".into(),
                ));
            }
            memory
                .write(&mut store, GUEST_INPUT_PTR, input)
                .map_err(|e| SandboxError::Compute(format!("failed to write guest input: {e}")))?;

            let run: Func = instance
                .get_func(&mut store, "run")
                .ok_or_else(|| SandboxError::Compute("guest missing exported 'run'".into()))?;
            let mut results = [Val::I32(0)];
            run.call(&mut store, &[Val::I32(input.len() as i32)], &mut results)
                .map_err(|e| SandboxError::Compute(format!("guest 'run' trapped: {e}")))?;
            let out_len = match results[0] {
                Val::I32(n) => n as usize,
                other => {
                    return Err(SandboxError::Compute(format!(
                        "guest 'run' returned unexpected result: {other:?}"
                    )));
                }
            };

            if GUEST_OUTPUT_PTR + out_len > memory.data_size(&store) {
                return Err(SandboxError::Compute(
                    "guest output buffer overflow: result length exceeds guest memory".into(),
                ));
            }
            let mut out = vec![0u8; out_len];
            memory
                .read(&store, GUEST_OUTPUT_PTR, &mut out)
                .map_err(|e| SandboxError::Compute(format!("failed to read guest output: {e}")))?;
            Ok(out)
        }
    }
}

#[cfg(feature = "wasmtime-backend")]
pub use wasmtime_backend::WasmtimeCompute;

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_soma_capability::token::CapabilityToken;

    fn tok(class: &str, action: &str) -> CapabilityToken {
        CapabilityToken {
            subject: "r".into(),
            resource_class: class.into(),
            cohort_scope: vec!["*".into()],
            action: action.into(),
            expiry: u64::MAX,
            nonce: vec![],
            signature: vec![],
            graph_scope: None,
        }
    }

    #[test]
    fn test_scoped_execution_allows_matching_token() {
        let out = execute_capability_scoped(
            &tok("simulation_output", "export"),
            "simulation_output",
            "export",
            &PassthroughCompute,
            b"x",
        )
        .unwrap();
        assert_eq!(out, b"x");
    }

    #[test]
    fn test_scoped_execution_rejects_wrong_class() {
        let err = execute_capability_scoped(
            &tok("genomic_variant", "read"),
            "simulation_output",
            "export",
            &PassthroughCompute,
            b"x",
        );
        assert!(err.is_err());
    }

    #[cfg(feature = "wasmtime-backend")]
    mod wasmtime_tests {
        use super::*;

        /// Minimal guest that reverses its input, proving real execution
        /// (not a passthrough): host writes at 1024, guest writes result at
        /// 2048, returns the length.
        const REVERSE_WAT: &str = r#"
            (module
              (memory (export "memory") 2)
              (func (export "run") (param $len i32) (result i32)
                (local $i i32)
                (local $src i32)
                (local $dst i32)
                (local.set $i (i32.const 0))
                (block $done
                  (loop $loop
                    (br_if $done (i32.ge_s (local.get $i) (local.get $len)))
                    (local.set $src (i32.add (i32.const 1024) (local.get $i)))
                    (local.set $dst
                      (i32.add (i32.const 2048)
                        (i32.sub (i32.sub (local.get $len) (i32.const 1)) (local.get $i))))
                    (i32.store8 (local.get $dst) (i32.load8_u (local.get $src)))
                    (local.set $i (i32.add (local.get $i) (i32.const 1)))
                    (br $loop)
                  )
                )
                (local.get $len)
              )
            )
        "#;

        #[test]
        fn wasmtime_backend_executes_guest_and_reverses() {
            let backend = WasmtimeCompute::new(REVERSE_WAT.as_bytes()).unwrap();
            let out = execute_capability_scoped(
                &tok("simulation_output", "export"),
                "simulation_output",
                "export",
                &backend,
                b"abc",
            )
            .unwrap();
            assert_eq!(out, b"cba");
        }

        #[test]
        fn wasmtime_backend_handles_empty_input() {
            let backend = WasmtimeCompute::new(REVERSE_WAT.as_bytes()).unwrap();
            let out = execute_capability_scoped(
                &tok("simulation_output", "export"),
                "simulation_output",
                "export",
                &backend,
                b"",
            )
            .unwrap();
            assert_eq!(out, b"");
        }

        #[test]
        fn wasmtime_backend_rejects_wrong_class_through_host_gate() {
            let backend = WasmtimeCompute::new(REVERSE_WAT.as_bytes()).unwrap();
            let err = execute_capability_scoped(
                &tok("genomic_variant", "read"),
                "simulation_output",
                "export",
                &backend,
                b"abc",
            );
            assert!(err.is_err());
        }

        #[test]
        fn wasmtime_backend_errors_on_missing_run_export() {
            const NO_RUN_WAT: &str = "(module (memory (export \"memory\") 1))";
            let backend = WasmtimeCompute::new(NO_RUN_WAT.as_bytes()).unwrap();
            let err = execute_capability_scoped(
                &tok("simulation_output", "export"),
                "simulation_output",
                "export",
                &backend,
                b"abc",
            );
            assert!(err.is_err());
        }
    }
}
