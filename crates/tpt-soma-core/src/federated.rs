//! Federated compute: capability-scoped result envelope (Phase 4).
//!
//! Partner sites return model weights / aggregate results in tamper-evident
//! envelopes authenticated with a key derived from the issuing capability
//! token's nonce and the site id. The payload may be encrypted upstream; this
//! module provides the integrity + capability-scoping envelope. Reuse of
//! Keystone's `wasmtime`-sandboxed UDFs for the compute itself is preferred
//! over building a separate sandbox.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// A capability-scoped, tamper-evident result returned by a federated site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedResultEnvelope {
    pub site_id: String,
    pub run_id: String,
    /// Either the plaintext aggregate or an encrypted blob (encryption is the
    /// site's responsibility; this envelope guarantees integrity + scope).
    pub payload: Vec<u8>,
    pub hmac: String,
}

/// Derive a symmetric scope key from the capability nonce + site id.
pub fn derive_scope_key(capability_nonce: &[u8], site_id: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(capability_nonce);
    hasher.update(site_id.as_bytes());
    hasher.finalize().to_vec()
}

fn compute_hmac(key: &[u8], site_id: &str, run_id: &str, payload: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(site_id.as_bytes());
    mac.update(run_id.as_bytes());
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

/// Sign a result into a capability-scoped envelope.
pub fn sign_envelope(
    key: &[u8],
    site_id: &str,
    run_id: &str,
    payload: &[u8],
) -> FederatedResultEnvelope {
    let hmac = compute_hmac(key, site_id, run_id, payload);
    FederatedResultEnvelope {
        site_id: site_id.to_string(),
        run_id: run_id.to_string(),
        payload: payload.to_vec(),
        hmac,
    }
}

/// Constant-time verification of an envelope against the scope key.
pub fn verify_envelope(key: &[u8], env: &FederatedResultEnvelope) -> bool {
    let expected = compute_hmac(key, &env.site_id, &env.run_id, &env.payload);
    if expected.len() != env.hmac.len() {
        return false;
    }
    let a = expected.as_bytes();
    let b = env.hmac.as_bytes();
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_verify_roundtrip() {
        let key = derive_scope_key(b"nonce-bytes", "site-a");
        let env = sign_envelope(&key, "site-a", "run-1", b"aggregate-weights");
        assert!(verify_envelope(&key, &env));
    }

    #[test]
    fn test_verify_fails_on_tamper() {
        let key = derive_scope_key(b"nonce-bytes", "site-a");
        let mut env = sign_envelope(&key, "site-a", "run-1", b"aggregate-weights");
        env.payload[0] ^= 0xFF;
        assert!(!verify_envelope(&key, &env));
    }

    #[test]
    fn test_verify_fails_on_wrong_key() {
        let key = derive_scope_key(b"nonce-bytes", "site-a");
        let env = sign_envelope(&key, "site-a", "run-1", b"aggregate-weights");
        let wrong = derive_scope_key(b"other-nonce", "site-a");
        assert!(!verify_envelope(&wrong, &env));
    }
}
