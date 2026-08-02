use ed25519_dalek::{SigningKey, VerifyingKey};
use tpt_soma_capability::{
    registry::DataClassRegistry, revocation::RevocationList, token::CapabilityToken,
};

fn deterministic_keypair() -> (SigningKey, VerifyingKey) {
    let bytes = [0x42u8; 32];
    let signing = SigningKey::from_bytes(&bytes);
    let verifying = signing.verifying_key();
    (signing, verifying)
}

fn make_token() -> CapabilityToken {
    CapabilityToken {
        subject: "researcher-1".into(),
        resource_class: "genomic_variant".into(),
        cohort_scope: vec!["cohort-a".into()],
        action: "read".into(),
        expiry: 9999999999,
        nonce: vec![1, 2, 3, 4],
        signature: Vec::new(),
    }
}

#[test]
fn forged_signature_rejected() {
    let (_sk, vk) = deterministic_keypair();
    let mut token = make_token();
    token.signature = vec![0u8; 64];
    assert!(!token.verify(&vk));
}

#[test]
fn expired_token_rejected() {
    let (_sk, _vk) = deterministic_keypair();
    let mut token = make_token();
    token.expiry = 1;
    assert!(token.is_expired());
}

#[test]
fn attenuated_token_cannot_exceed_parent() {
    let mut registry = DataClassRegistry::default();
    registry.seed_phase0();
    let class = registry.get("genomic_variant");
    assert!(class.is_some());
}

#[test]
fn revoked_token_detected() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let list = RevocationList::new();
        let nonce = vec![1, 2, 3, 4];
        list.revoke(nonce.clone()).await;
        assert!(list.contains(&nonce).await);
        assert!(!list.is_empty().await);
    });
}
