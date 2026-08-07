//! tpt-soma-sandbox: host API surface for researcher-submitted compute
//! (Phase 4, best-effort scaffold).
//!
//! This crate defines the *host API surface* that researcher-submitted compute
//! must go through, gated by the same capability tokens used everywhere else in
//! `tpt-soma`. Actual code isolation should reuse Keystone's existing
//! `wasmtime`-sandboxed WASM UDFs rather than building a second sandbox; the
//! [`ResearcherCompute`] trait is the seam where that execution backend plugs
//! in (or a no-op/local backend for tests).

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
/// run WASM via Keystone's wasmtime backend; the trait keeps the host API
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
}
