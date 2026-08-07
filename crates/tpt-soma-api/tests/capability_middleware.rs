use axum::{Router, http::StatusCode, routing::get};
use ed25519_dalek::SigningKey;
use rand::RngCore;
use std::net::SocketAddr;
use std::sync::Arc;
use tpt_soma_api::auth::{AuthState, capability_middleware};
use tpt_soma_audit::AuditLedger;
use tpt_soma_capability::{CapabilityToken, signing::LocalSigningBackend};
use tpt_soma_core::connection::{create_pool, run_migrations};
use tpt_soma_core::dp::DifferentialPrivacyService;

async fn build_test_app() -> Result<(Router, SocketAddr), Box<dyn std::error::Error + Send + Sync>>
{
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/tpt_soma_test".to_string()
    });

    let pool = create_pool(&database_url).await?;
    run_migrations(&pool).await?;

    let mut csprng = rand::thread_rng();
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let verifying_key = signing_key.verifying_key();

    let revocation_list = Arc::new(tpt_soma_capability::RevocationList::new());
    let audit_ledger = Arc::new(AuditLedger::new(pool.clone()));
    let dp_service = Arc::new(tokio::sync::Mutex::new(DifferentialPrivacyService::new(
        1.0,
    )));

    let object_store = Arc::new(tpt_soma_core::store::ObjectStoreClient::from_env());

    let auth_state = Arc::new(AuthState {
        pool,
        verifying_key,
        revocation_list,
        audit_ledger,
        dp_service,
        object_store,
    });

    let app = Router::new()
        .route("/protected", get(|| async { "secret data" }))
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            capability_middleware,
        ))
        .with_state(auth_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let app_clone = app.clone();
    tokio::spawn(async move {
        axum::serve(listener, app_clone.into_make_service())
            .await
            .ok();
    });

    Ok((app, addr))
}

fn make_token(signing_key: &SigningKey) -> String {
    let backend = LocalSigningBackend::new(signing_key.clone());
    let token = CapabilityToken {
        subject: "test-researcher".to_string(),
        resource_class: "genomic_variant".to_string(),
        cohort_scope: vec!["cohort-a".to_string()],
        action: "read".to_string(),
        expiry: u64::MAX,
        nonce: [42u8; 32].to_vec(),
        signature: Vec::new(),
        graph_scope: None,
    };
    let signed = CapabilityToken::sign(&backend, token);
    serde_json::to_string(&signed).unwrap()
}

async fn make_request(addr: SocketAddr, auth_header: Option<&str>) -> reqwest::Response {
    let client = reqwest::Client::new();
    let mut request = client.get(format!("http://{}/protected", addr));
    if let Some(header) = auth_header {
        request = request.header("Authorization", header);
    }
    request.send().await.unwrap()
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_missing_auth_header_rejected() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let (_, addr) = build_test_app().await?;
    let response = make_request(addr, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_invalid_token_format_rejected() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let (_, addr) = build_test_app().await?;
    let response = make_request(addr, Some("Bearer not-a-json-token")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_forged_signature_rejected() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (_, addr) = build_test_app().await?;

    let mut csprng = rand::thread_rng();
    let mut other_key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut other_key_bytes);
    let other_signing_key = SigningKey::from_bytes(&other_key_bytes);

    let forged_token = make_token(&other_signing_key);
    let response = make_request(addr, Some(&format!("Bearer {}", forged_token))).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_valid_token_accepted() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (_, addr) = build_test_app().await?;

    let mut csprng = rand::thread_rng();
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);

    let valid_token = make_token(&signing_key);
    let response = make_request(addr, Some(&format!("Bearer {}", valid_token))).await;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}
