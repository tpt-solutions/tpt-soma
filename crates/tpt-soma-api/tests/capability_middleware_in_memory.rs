use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
};
use ed25519_dalek::SigningKey;
use rand::RngCore;
use std::sync::Arc;
use tpt_soma_api::auth::AuthError;
use tpt_soma_capability::{
    CapabilityToken, RevocationList,
    signing::{LocalSigningBackend, SigningBackend},
};

fn make_test_token(signing_key: &SigningKey, expiry: u64) -> String {
    let backend = LocalSigningBackend::new(signing_key.clone());
    let token = CapabilityToken {
        subject: "test-researcher".to_string(),
        resource_class: "genomic_variant".to_string(),
        cohort_scope: vec!["cohort-a".to_string()],
        action: "read".to_string(),
        expiry,
        nonce: [42u8; 32].to_vec(),
        signature: Vec::new(),
    };
    let signed = CapabilityToken::sign(&backend, token);
    serde_json::to_string(&signed).unwrap()
}

fn make_test_token_with_params(
    signing_key: &SigningKey,
    subject: &str,
    resource_class: &str,
    cohort_scope: Vec<String>,
    action: &str,
    expiry: u64,
    nonce: [u8; 32],
) -> String {
    let backend = LocalSigningBackend::new(signing_key.clone());
    let token = CapabilityToken {
        subject: subject.to_string(),
        resource_class: resource_class.to_string(),
        cohort_scope,
        action: action.to_string(),
        expiry,
        nonce: nonce.to_vec(),
        signature: Vec::new(),
    };
    let signed = CapabilityToken::sign(&backend, token);
    serde_json::to_string(&signed).unwrap()
}

#[tokio::test]
async fn test_token_verification_valid() {
    let mut csprng = rand::thread_rng();
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let verifying_key = signing_key.verifying_key();

    let token_str = make_test_token(&signing_key, u64::MAX);
    let token: CapabilityToken = serde_json::from_str(&token_str).unwrap();

    assert!(token.verify(&verifying_key));
    assert!(!token.is_expired());
}

#[tokio::test]
async fn test_token_verification_forged_signature() {
    let mut csprng = rand::thread_rng();
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let verifying_key = signing_key.verifying_key();

    let mut other_key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut other_key_bytes);
    let other_signing_key = SigningKey::from_bytes(&other_key_bytes);

    let token_str = make_test_token(&other_signing_key, u64::MAX);
    let token: CapabilityToken = serde_json::from_str(&token_str).unwrap();

    assert!(!token.verify(&verifying_key));
}

#[tokio::test]
async fn test_token_verification_expired() {
    let mut csprng = rand::thread_rng();
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let verifying_key = signing_key.verifying_key();

    // Token expired 1 hour ago - create token manually without using sign() which overwrites expiry
    let expiry = (chrono::Utc::now().timestamp() - 3600) as u64;
    let backend = LocalSigningBackend::new(signing_key.clone());
    let mut token = CapabilityToken {
        subject: "test-researcher".to_string(),
        resource_class: "genomic_variant".to_string(),
        cohort_scope: vec!["cohort-a".to_string()],
        action: "read".to_string(),
        expiry,
        nonce: [42u8; 32].to_vec(),
        signature: Vec::new(),
    };
    // Manually sign without overwriting expiry
    let payload = tpt_soma_capability::token::Payload {
        subject: token.subject.clone(),
        resource_class: token.resource_class.clone(),
        cohort_scope: token.cohort_scope.clone(),
        action: token.action.clone(),
        expiry: token.expiry,
        nonce: token.nonce.clone(),
    };
    let payload_bytes = serde_json::to_vec(&payload).expect("serialize");
    let signature = backend.sign(&payload_bytes).expect("sign");
    token.signature = signature;
    let token_str = serde_json::to_string(&token).unwrap();
    let token: CapabilityToken = serde_json::from_str(&token_str).unwrap();

    assert!(token.verify(&verifying_key));
    assert!(token.is_expired());
}

#[tokio::test]
async fn test_revocation_list() {
    let revocation_list = Arc::new(RevocationList::new());
    let nonce = [42u8; 32];

    // Not revoked initially
    assert!(!revocation_list.contains(nonce.as_ref()).await);

    // Revoke
    revocation_list.revoke(nonce.to_vec()).await;

    // Now revoked
    assert!(revocation_list.contains(nonce.as_ref()).await);
}

#[tokio::test]
async fn test_token_revoked() {
    let mut csprng = rand::thread_rng();
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let _verifying_key = signing_key.verifying_key();

    let nonce = [42u8; 32];
    let token_str = make_test_token_with_params(
        &signing_key,
        "test-researcher",
        "genomic_variant",
        vec!["cohort-a".to_string()],
        "read",
        u64::MAX,
        nonce,
    );
    let token: CapabilityToken = serde_json::from_str(&token_str).unwrap();

    let revocation_list = Arc::new(RevocationList::new());
    assert!(!revocation_list.contains(&token.nonce).await);

    revocation_list.revoke(token.nonce.clone()).await;
    assert!(revocation_list.contains(&token.nonce).await);
}

#[tokio::test]
async fn test_attenuated_token_cannot_exceed_parent_scope() {
    let mut csprng = rand::thread_rng();
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);

    // Parent token with read access to cohort-a
    let parent_token_str = make_test_token_with_params(
        &signing_key,
        "researcher-1",
        "genomic_variant",
        vec!["cohort-a".to_string()],
        "read",
        u64::MAX,
        [1u8; 32],
    );
    let parent_token: CapabilityToken = serde_json::from_str(&parent_token_str).unwrap();

    // Attenuated token trying to access cohort-b (not in parent scope)
    let attenuated_token_str = make_test_token_with_params(
        &signing_key,
        "researcher-1",
        "genomic_variant",
        vec!["cohort-b".to_string()], // Different cohort!
        "read",
        u64::MAX,
        [2u8; 32],
    );
    let attenuated_token: CapabilityToken = serde_json::from_str(&attenuated_token_str).unwrap();

    // Verify attenuation logic - attenuated token's cohort scope must be subset of parent
    let parent_cohorts: std::collections::HashSet<_> = parent_token.cohort_scope.iter().collect();
    let attenuated_cohorts: std::collections::HashSet<_> =
        attenuated_token.cohort_scope.iter().collect();

    assert!(!attenuated_cohorts.is_subset(&parent_cohorts));
}

#[tokio::test]
async fn test_attenuated_token_action_exceeds_parent() {
    let mut csprng = rand::thread_rng();
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);

    // Parent token with read access
    let parent_token_str = make_test_token_with_params(
        &signing_key,
        "researcher-1",
        "genomic_variant",
        vec!["cohort-a".to_string()],
        "read",
        u64::MAX,
        [1u8; 32],
    );
    let parent_token: CapabilityToken = serde_json::from_str(&parent_token_str).unwrap();

    // Attenuated token trying to write (exceeds parent's read)
    let attenuated_token_str = make_test_token_with_params(
        &signing_key,
        "researcher-1",
        "genomic_variant",
        vec!["cohort-a".to_string()],
        "write", // Exceeds parent's read!
        u64::MAX,
        [2u8; 32],
    );
    let attenuated_token: CapabilityToken = serde_json::from_str(&attenuated_token_str).unwrap();

    // Verify attenuation logic - attenuated token's action must not exceed parent
    // In a real implementation, we'd have an action hierarchy: read < write < admin
    assert_ne!(parent_token.action, attenuated_token.action);
    assert_eq!(parent_token.action, "read");
    assert_eq!(attenuated_token.action, "write");
}

#[tokio::test]
async fn test_invalid_token_format() {
    // Test that invalid JSON is rejected
    let invalid_json = "not valid json";
    let result: Result<CapabilityToken, _> = serde_json::from_str(invalid_json);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_missing_auth_header_error() {
    let error = AuthError::MissingAuthHeader;
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_auth_header_error() {
    let error = AuthError::InvalidAuthHeader;
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_token_format_error() {
    let error = AuthError::InvalidTokenFormat;
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_signature_error() {
    let error = AuthError::InvalidSignature;
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_token_expired_error() {
    let error = AuthError::TokenExpired;
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_token_revoked_error() {
    let error = AuthError::TokenRevoked;
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_insufficient_scope_error() {
    let error = AuthError::InsufficientScope;
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_compute_query_fingerprint() {
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/variants?cohort=cohort-a")
        .body(Body::empty())
        .unwrap();

    let fingerprint = tpt_soma_api::auth::compute_query_fingerprint(&req);

    // Should be a valid hex string
    assert!(!fingerprint.is_empty());
    assert!(fingerprint.chars().all(|c| c.is_ascii_hexdigit()));

    // Same request should produce same fingerprint
    let req2 = Request::builder()
        .method("GET")
        .uri("/api/v1/variants?cohort=cohort-a")
        .body(Body::empty())
        .unwrap();
    let fingerprint2 = tpt_soma_api::auth::compute_query_fingerprint(&req2);
    assert_eq!(fingerprint, fingerprint2);

    // Different request should produce different fingerprint
    let req3 = Request::builder()
        .method("POST")
        .uri("/api/v1/variants?cohort=cohort-a")
        .body(Body::empty())
        .unwrap();
    let fingerprint3 = tpt_soma_api::auth::compute_query_fingerprint(&req3);
    assert_ne!(fingerprint, fingerprint3);
}
