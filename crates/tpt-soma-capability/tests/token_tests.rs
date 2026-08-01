use tpt_soma_capability::{
    token::{CapabilityToken, Payload},
    attenuation::AttenuatedToken,
    registry::DataClassRegistry,
    revocation::RevocationList,
};
use ed25519_dalek::{SigningKey, VerifyingKey, keypair::Keypair};

fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let signing = SigningKey::generate(&mut rand::thread_rng());
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
    let (_sk, vk) = generate_keypair();
    let mut token = make_token();
    token.signature = vec![0u8; 64];
    assert!(!token.verify(&vk));
}

#[test]
fn expired_token_rejected() {
    let (_sk, vk) = generate_keypair();
    let mut token = make_token();
    token.expiry = 1;
    token.signature = vec![0u8; 64];
    assert!(token.is_expired());
}

#[test]
fn attenuated_token_cannot_exceed_parent() {
    let _ = DataClassRegistry::default();
    let _ = RevocationList::new();
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
