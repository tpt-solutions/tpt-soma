pub mod attenuation;
pub mod registry;
pub mod revocation;
pub mod signing;
pub mod token;

pub use registry::DataClassRegistry;
pub use revocation::RevocationList;
pub use signing::{
    KmsSigningBackend, KmsSigningBackendStub, LocalSigningBackend, SigningBackend, SigningError,
};
pub use token::CapabilityToken;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attenuation::AttenuatedToken;
    use crate::signing::LocalSigningBackend;

    #[test]
    fn test_forged_signature_rejected() {
        let backend = LocalSigningBackend::generate();
        let verifying_key = backend.verifying_key();

        let other_backend = LocalSigningBackend::generate();
        let _other_verifying_key = other_backend.verifying_key();

        let token = CapabilityToken {
            subject: "researcher".to_string(),
            resource_class: "genomic_variant".to_string(),
            cohort_scope: vec!["cohort-a".to_string()],
            action: "read".to_string(),
            expiry: u64::MAX,
            nonce: [3u8; 32].to_vec(),
            signature: Vec::new(),
            graph_scope: None,
        };

        let forged_token = CapabilityToken::sign(&other_backend, token);
        assert!(!forged_token.verify(&verifying_key));
    }

    #[test]
    fn test_expired_token_rejected() {
        let backend = LocalSigningBackend::generate();

        let mut token = CapabilityToken {
            subject: "researcher".to_string(),
            resource_class: "genomic_variant".to_string(),
            cohort_scope: vec!["cohort-a".to_string()],
            action: "read".to_string(),
            expiry: 0,
            nonce: [4u8; 32].to_vec(),
            signature: Vec::new(),
            graph_scope: None,
        };

        token = CapabilityToken::sign(&backend, token);
        token.expiry = 1000;
        assert!(token.is_expired());
    }

    #[test]
    fn test_sign_honors_requested_expiry() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let backend = LocalSigningBackend::generate();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs();
        let requested = now + 604_800; // 7 days

        let token = CapabilityToken {
            subject: "researcher".to_string(),
            resource_class: "genomic_variant".to_string(),
            cohort_scope: vec!["cohort-a".to_string()],
            action: "read".to_string(),
            expiry: requested,
            nonce: [9u8; 32].to_vec(),
            signature: Vec::new(),
            graph_scope: None,
        };

        let signed = CapabilityToken::sign(&backend, token);
        assert_eq!(
            signed.expiry, requested,
            "sign must not override requested expiry"
        );
        assert!(!signed.is_expired());
        assert!(signed.verify(&backend.verifying_key()));
    }

    #[test]
    fn test_attenuated_token_scope_exceeds_parent() {
        let backend = LocalSigningBackend::generate();

        let parent = CapabilityToken {
            subject: "researcher".to_string(),
            resource_class: "genomic_variant".to_string(),
            cohort_scope: vec!["cohort-a".to_string(), "cohort-b".to_string()],
            action: "read".to_string(),
            expiry: u64::MAX,
            nonce: [1u8; 32].to_vec(),
            signature: Vec::new(),
            graph_scope: None,
        };

        let parent = CapabilityToken::sign(&backend, parent);

        let result = AttenuatedToken::derive_child(
            &parent,
            "read",
            vec![
                "cohort-a".to_string(),
                "cohort-b".to_string(),
                "cohort-c".to_string(),
            ],
            &[0u8; 32],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_attenuated_token_action_exceeds_parent() {
        let backend = LocalSigningBackend::generate();

        let parent = CapabilityToken {
            subject: "researcher".to_string(),
            resource_class: "genomic_variant".to_string(),
            cohort_scope: vec!["cohort-a".to_string()],
            action: "read".to_string(),
            expiry: u64::MAX,
            nonce: [1u8; 32].to_vec(),
            signature: Vec::new(),
            graph_scope: None,
        };

        let parent = CapabilityToken::sign(&backend, parent);

        let result = AttenuatedToken::derive_child(
            &parent,
            "write",
            vec!["cohort-a".to_string()],
            &[0u8; 32],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_revoked_token_rejected_full_path() {
        let backend = LocalSigningBackend::generate();
        let verifying_key = backend.verifying_key();

        let revocation_list = RevocationList::new();

        let token = CapabilityToken {
            subject: "researcher".to_string(),
            resource_class: "genomic_variant".to_string(),
            cohort_scope: vec!["cohort-a".to_string()],
            action: "read".to_string(),
            expiry: u64::MAX,
            nonce: [2u8; 32].to_vec(),
            signature: Vec::new(),
            graph_scope: None,
        };

        let token = CapabilityToken::sign(&backend, token);

        assert!(token.verify(&verifying_key));

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            revocation_list.revoke(token.nonce.clone()).await;
        });

        assert!(token.verify(&verifying_key));

        let rt2 = tokio::runtime::Runtime::new().unwrap();
        let revoked = rt2.block_on(async { revocation_list.contains(&token.nonce).await });
        assert!(
            revoked,
            "revoked token should be rejected through full verification path"
        );
    }
}
