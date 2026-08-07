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

/// A tamper-evident binding between a federated result envelope and the
/// central audit ledger's root of trust (its tail `row_hash`). Partner sites
/// attach this to their envelope so the central site can prove the result was
/// produced against a known ledger state — the consistency-proof path that
/// reconciles local-site ledgers against the central ledger (ADR 007 §2.7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerConsistencyProof {
    /// Tail `row_hash` of the central audit ledger the envelope is bound to.
    pub central_ledger_hash: String,
    /// The envelope's own HMAC, captured so the proof is self-describing.
    pub envelope_hmac: String,
    /// HMAC over `(central_ledger_hash || envelope_hmac)` under the scope key.
    pub proof_hmac: String,
}

fn compute_proof_hmac(key: &[u8], central_ledger_hash: &str, envelope_hmac: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(central_ledger_hash.as_bytes());
    mac.update(envelope_hmac.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Bind `env` to the central ledger's tail hash, producing a consistency proof.
pub fn prove_ledger_consistency(
    key: &[u8],
    central_ledger_hash: &str,
    env: &FederatedResultEnvelope,
) -> LedgerConsistencyProof {
    let proof_hmac = compute_proof_hmac(key, central_ledger_hash, &env.hmac);
    LedgerConsistencyProof {
        central_ledger_hash: central_ledger_hash.to_string(),
        envelope_hmac: env.hmac.clone(),
        proof_hmac,
    }
}

/// Verify a consistency proof against the scope key. Returns `true` only if the
/// envelope HMAC and central ledger hash in the proof are bound by a valid
/// HMAC under `key`. (The envelope payload itself is re-verified separately via
/// [`verify_envelope`] when the site also supplies the payload.)
pub fn verify_ledger_consistency(key: &[u8], proof: &LedgerConsistencyProof) -> bool {
    let expected = compute_proof_hmac(key, &proof.central_ledger_hash, &proof.envelope_hmac);
    if expected.len() != proof.proof_hmac.len() {
        return false;
    }
    let a = expected.as_bytes();
    let b = proof.proof_hmac.as_bytes();
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

    #[test]
    fn test_ledger_consistency_proof_roundtrip() {
        let key = derive_scope_key(b"nonce-bytes", "site-a");
        let env = sign_envelope(&key, "site-a", "run-1", b"aggregate-weights");
        let proof = prove_ledger_consistency(&key, "central-tail-hash", &env);
        assert!(verify_ledger_consistency(&key, &proof));
        assert_eq!(proof.central_ledger_hash, "central-tail-hash");
    }

    #[test]
    fn test_ledger_consistency_fails_on_tampered_ledger_hash() {
        let key = derive_scope_key(b"nonce-bytes", "site-a");
        let env = sign_envelope(&key, "site-a", "run-1", b"aggregate-weights");
        let mut proof = prove_ledger_consistency(&key, "central-tail-hash", &env);
        proof.central_ledger_hash = "forged-hash".to_string();
        assert!(!verify_ledger_consistency(&key, &proof));
    }

    #[test]
    fn test_ledger_consistency_fails_on_wrong_key() {
        let key = derive_scope_key(b"nonce-bytes", "site-a");
        let env = sign_envelope(&key, "site-a", "run-1", b"aggregate-weights");
        let proof = prove_ledger_consistency(&key, "central-tail-hash", &env);
        let wrong = derive_scope_key(b"other-nonce", "site-a");
        assert!(!verify_ledger_consistency(&wrong, &proof));
    }
}
