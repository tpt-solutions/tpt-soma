use axum::{
    extract::{Request, State},
    http::{StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tpt_soma_audit::{AuditEvent, AuditLedger};
use tpt_soma_capability::{CapabilityToken, RevocationList};
use tpt_soma_core::connection::PgPool;
use uuid::Uuid;
use chrono::Utc;

#[derive(Clone)]
pub struct AuthState {
    pub pool: PgPool,
    pub verifying_key: ed25519_dalek::VerifyingKey,
    pub revocation_list: Arc<RevocationList>,
    pub audit_ledger: Arc<AuditLedger>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub subject: String,
    pub resource_class: String,
    pub cohort_scope: Vec<String>,
    pub action: String,
    pub expiry: u64,
    pub nonce: Vec<u8>,
}

pub async fn capability_middleware(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .ok_or(AuthError::MissingAuthHeader)?;

    let auth_str = auth_header.to_str().map_err(|_| AuthError::InvalidAuthHeader)?;
    let token_str = auth_str
        .strip_prefix("Bearer ")
        .ok_or(AuthError::InvalidAuthHeader)?;

    let token: CapabilityToken = serde_json::from_str(token_str)
        .map_err(|_| AuthError::InvalidTokenFormat)?;

    // Verify token signature
    if !token.verify(&state.verifying_key) {
        return Err(AuthError::InvalidSignature);
    }

    // Check expiry
    if token.is_expired() {
        return Err(AuthError::TokenExpired);
    }

    // Check revocation
    if state.revocation_list.contains(&token.nonce).await {
        return Err(AuthError::TokenRevoked);
    }

    // Verify resource class is registered
    // (In production, this would check against the data_class_registry table)

    // Create audit event
    let audit_event = AuditEvent {
        id: Uuid::new_v4(),
        actor: token.subject.clone(),
        resource_class: token.resource_class.clone(),
        action: token.action.clone(),
        cohort_scope: token.cohort_scope.clone(),
        timestamp: Utc::now(),
        query_fingerprint: compute_query_fingerprint(&req),
        outcome: "pending".to_string(),
        prev_row_hash: None, // Will be filled by audit ledger
        row_hash: String::new(), // Will be filled by audit ledger
    };

    // Store audit event ID in request extensions for later use
    req.extensions_mut().insert(audit_event.id);
    req.extensions_mut().insert(token);

    let response = next.run(req).await;

    // Update audit event with outcome
    let mut audit_event = audit_event;
    audit_event.outcome = if response.status().is_success() {
        "success".to_string()
    } else {
        "failure".to_string()
    };

    // Write to audit ledger (fire and forget for performance)
    let audit_ledger = state.audit_ledger.clone();
    tokio::spawn(async move {
        let _ = audit_ledger.append(audit_event).await;
    });

    Ok(response)
}

fn compute_query_fingerprint(req: &Request) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(req.method().as_str().as_bytes());
    hasher.update(req.uri().path().as_bytes());
    if let Some(query) = req.uri().query() {
        hasher.update(query.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("missing authorization header")]
    MissingAuthHeader,
    #[error("invalid authorization header format")]
    InvalidAuthHeader,
    #[error("invalid token format")]
    InvalidTokenFormat,
    #[error("invalid token signature")]
    InvalidSignature,
    #[error("token expired")]
    TokenExpired,
    #[error("token revoked")]
    TokenRevoked,
    #[error("insufficient scope")]
    InsufficientScope,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingAuthHeader => (StatusCode::UNAUTHORIZED, "Missing authorization header"),
            AuthError::InvalidAuthHeader => (StatusCode::UNAUTHORIZED, "Invalid authorization header"),
            AuthError::InvalidTokenFormat => (StatusCode::UNAUTHORIZED, "Invalid token format"),
            AuthError::InvalidSignature => (StatusCode::UNAUTHORIZED, "Invalid token signature"),
            AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "Token expired"),
            AuthError::TokenRevoked => (StatusCode::UNAUTHORIZED, "Token revoked"),
            AuthError::InsufficientScope => (StatusCode::FORBIDDEN, "Insufficient scope"),
        };
        (status, message).into_response()
    }
}