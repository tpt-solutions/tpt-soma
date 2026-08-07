use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tpt_soma_audit::{AuditEvent, AuditLedger};
use tpt_soma_capability::{CapabilityToken, RevocationList};
use tpt_soma_core::connection::PgPool;
use tpt_soma_core::store::ObjectStoreClient;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthState {
    pub pool: PgPool,
    pub verifying_key: ed25519_dalek::VerifyingKey,
    pub revocation_list: Arc<RevocationList>,
    pub audit_ledger: Arc<AuditLedger>,
    pub dp_service: Arc<tokio::sync::Mutex<tpt_soma_core::DifferentialPrivacyService>>,
    pub object_store: Arc<ObjectStoreClient>,
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

pub async fn record_dp_budget_spend(
    audit_ledger: &Arc<AuditLedger>,
    actor: &str,
    cohort: &str,
    epsilon_spent: f64,
) {
    let event = AuditEvent {
        id: Uuid::new_v4(),
        actor: actor.to_string(),
        resource_class: "dp_budget".to_string(),
        action: "spend".to_string(),
        cohort_scope: vec![cohort.to_string()],
        timestamp: Utc::now(),
        query_fingerprint: format!(
            "dp_budget_spend:cohort={}:epsilon={}",
            cohort, epsilon_spent
        ),
        outcome: "success".to_string(),
        prev_row_hash: None,
        row_hash: String::new(),
    };
    let _ = audit_ledger.append(event).await;
}

pub async fn record_dp_budget_spend_with_actor(
    audit_ledger: &Arc<AuditLedger>,
    actor: &str,
    cohort: &str,
    _epsilon_spent: f64,
    query_fingerprint: &str,
) {
    let event = AuditEvent {
        id: Uuid::new_v4(),
        actor: actor.to_string(),
        resource_class: "dp_budget".to_string(),
        action: "spend".to_string(),
        cohort_scope: vec![cohort.to_string()],
        timestamp: Utc::now(),
        query_fingerprint: query_fingerprint.to_string(),
        outcome: "success".to_string(),
        prev_row_hash: None,
        row_hash: String::new(),
    };
    let _ = audit_ledger.append(event).await;
}

pub async fn authenticate_bearer(
    auth_header: Option<&str>,
    verifying_key: &ed25519_dalek::VerifyingKey,
    revocation_list: &Arc<RevocationList>,
) -> Result<CapabilityToken, AuthError> {
    let auth_str = auth_header.ok_or(AuthError::MissingAuthHeader)?;
    let token_str = auth_str
        .strip_prefix("Bearer ")
        .ok_or(AuthError::InvalidAuthHeader)?;

    let token: CapabilityToken =
        serde_json::from_str(token_str).map_err(|_| AuthError::InvalidTokenFormat)?;

    if !token.verify(verifying_key) {
        return Err(AuthError::InvalidSignature);
    }

    if token.is_expired() {
        return Err(AuthError::TokenExpired);
    }

    if revocation_list.contains(&token.nonce).await {
        return Err(AuthError::TokenRevoked);
    }

    Ok(token)
}

/// Route-level authorization policy (TM-01 fix): every protected route maps to
/// the set of resource classes and the minimum action it may touch.
pub struct RequiredCapability {
    pub allowed_classes: &'static [&'static str],
    pub action: &'static str,
}

const POLICY: &[(&str, &str, &[&str], &str)] = &[
    // (method, path-pattern, allowed resource classes, minimum action)
    ("GET", "/api/v1/variants/*", &["genomic_variant"], "read"),
    ("GET", "/api/v1/variants/*/*", &["genomic_variant"], "read"),
    (
        "GET",
        "/api/v1/expression/*",
        &["transcriptomic_scrna"],
        "read",
    ),
    (
        "GET",
        "/api/v1/expression/gene/*",
        &["transcriptomic_scrna"],
        "read",
    ),
    ("GET", "/api/v1/umap/*", &["transcriptomic_scrna"], "read"),
    (
        "GET",
        "/api/v1/umap/*/cluster/*",
        &["transcriptomic_scrna"],
        "read",
    ),
    (
        "POST",
        "/api/v1/join/variant-expression",
        &["genomic_variant", "transcriptomic_scrna"],
        "read",
    ),
    (
        "GET",
        "/api/v1/cohorts/*/samples",
        &[
            "genomic_variant",
            "transcriptomic_scrna",
            "clinical_observation",
            "cgm_continuous",
        ],
        "read",
    ),
    (
        "POST",
        "/api/v1/cohorts/*/aggregate/count",
        &[
            "genomic_variant",
            "transcriptomic_scrna",
            "clinical_observation",
            "cgm_continuous",
        ],
        "export",
    ),
    (
        "POST",
        "/api/v1/cohorts/*/aggregate/cross-domain",
        &[
            "genomic_variant",
            "transcriptomic_scrna",
            "clinical_observation",
            "cgm_continuous",
            "simulation_output",
        ],
        "export",
    ),
    ("POST", "/api/v1/ingest/vcf", &["genomic_raw"], "write"),
    (
        "POST",
        "/api/v1/ingest/h5ad",
        &["transcriptomic_scrna"],
        "write",
    ),
    (
        "POST",
        "/api/v1/ingest/fhir-observation",
        &["clinical_observation"],
        "write",
    ),
    (
        "POST",
        "/api/v1/ingest/organ-csv",
        &["clinical_observation"],
        "write",
    ),
    (
        "GET",
        "/api/v1/clinical-observations/*",
        &["clinical_observation"],
        "read",
    ),
    (
        "GET",
        "/api/v1/clinical-observations/*/*/trajectory",
        &["clinical_observation"],
        "read",
    ),
    (
        "POST",
        "/api/v1/ingest/imaging",
        &["organ_imaging"],
        "write",
    ),
    ("GET", "/api/v1/organ-imaging/*", &["organ_imaging"], "read"),
    (
        "GET",
        "/api/v1/organ-system-graph",
        &["clinical_observation"],
        "read",
    ),
    (
        "POST",
        "/api/v1/organ-system-graph/cascade",
        &["clinical_observation"],
        "read",
    ),
    ("POST", "/api/v1/ingest/cgm", &["cgm_continuous"], "write"),
    ("GET", "/api/v1/cgm/*", &["cgm_continuous"], "read"),
    (
        "GET",
        "/api/v1/cgm/*/variability",
        &["cgm_continuous"],
        "read",
    ),
    (
        "GET",
        "/api/v1/subjects/*/cross-phase-summary",
        &[
            "genomic_variant",
            "transcriptomic_scrna",
            "clinical_observation",
            "cgm_continuous",
        ],
        "read",
    ),
    // Phase 3: simulation outputs (digital-twin runs)
    ("POST", "/api/v1/simulate", &["simulation_output"], "write"),
    (
        "GET",
        "/api/v1/simulations/*",
        &["simulation_output"],
        "read",
    ),
    (
        "POST",
        "/api/v1/simulations/*/aggregate/count",
        &["simulation_output"],
        "export",
    ),
];

fn path_matches(path: &str, pattern: &str) -> bool {
    let path_segs: Vec<&str> = path.split('/').collect();
    let pattern_segs: Vec<&str> = pattern.split('/').collect();
    if path_segs.len() != pattern_segs.len() {
        return false;
    }
    path_segs
        .iter()
        .zip(pattern_segs.iter())
        .all(|(a, b)| *b == "*" || a == b)
}

pub fn required_capability_for(method: &str, path: &str) -> Option<RequiredCapability> {
    POLICY
        .iter()
        .find(|(m, pattern, _, _)| *m == method && path_matches(path, pattern))
        .map(|(_, _, classes, action)| RequiredCapability {
            allowed_classes: classes,
            action,
        })
}

fn action_rank(action: &str) -> i32 {
    match action {
        "admin" => 4,
        "export" => 3,
        "write" => 2,
        "read" => 1,
        _ => 0,
    }
}

fn action_allows(token_action: &str, required: &str) -> bool {
    let token_rank = action_rank(token_action);
    token_rank > 0 && token_rank >= action_rank(required)
}

fn extract_cohort_id(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/cohorts/")?;
    let id = rest.split('/').next().unwrap_or("");
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Enforce the token's resource class, action, and cohort scope against the
/// route-level policy. Returns Ok(()) when the token satisfies the policy.
pub fn enforce_route_policy(
    token: &CapabilityToken,
    method: &str,
    path: &str,
) -> Result<(), AuthError> {
    let Some(required) = required_capability_for(method, path) else {
        return Ok(());
    };

    if !required
        .allowed_classes
        .contains(&token.resource_class.as_str())
    {
        return Err(AuthError::InsufficientScope);
    }

    if !action_allows(&token.action, required.action) {
        return Err(AuthError::InsufficientScope);
    }

    if let Some(cohort_id) = extract_cohort_id(path) {
        let in_scope = token
            .cohort_scope
            .iter()
            .any(|c| c == "*" || c == &cohort_id);
        if !in_scope {
            return Err(AuthError::InsufficientScope);
        }
    }

    Ok(())
}

/// Enforce a token's optional graph-traversal scope against a concrete OSG
/// node/edge id. A token without `graph_scope` (i.e. `None`) is not restricted
/// to specific graph entities; a token with a scope list may only touch ids in
/// that list (or `*`). This is the building block for graph-traversal-scoped
/// access (ADR 007 §2.8), applied by graph query endpoints before returning
/// topology.
pub fn graph_scope_allows(token: &CapabilityToken, entity_id: &str) -> bool {
    match &token.graph_scope {
        None => true,
        Some(scope) => scope.iter().any(|s| s == "*" || s == entity_id),
    }
}

pub async fn capability_middleware(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    let token =
        authenticate_bearer(auth_header, &state.verifying_key, &state.revocation_list).await?;

    // Route-level policy enforcement (resource class / action / cohort scope)
    enforce_route_policy(&token, req.method().as_str(), req.uri().path())?;

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
        prev_row_hash: None,     // Will be filled by audit ledger
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

pub fn compute_query_fingerprint(req: &Request) -> String {
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
            AuthError::MissingAuthHeader => {
                (StatusCode::UNAUTHORIZED, "Missing authorization header")
            }
            AuthError::InvalidAuthHeader => {
                (StatusCode::UNAUTHORIZED, "Invalid authorization header")
            }
            AuthError::InvalidTokenFormat => (StatusCode::UNAUTHORIZED, "Invalid token format"),
            AuthError::InvalidSignature => (StatusCode::UNAUTHORIZED, "Invalid token signature"),
            AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "Token expired"),
            AuthError::TokenRevoked => (StatusCode::UNAUTHORIZED, "Token revoked"),
            AuthError::InsufficientScope => (StatusCode::FORBIDDEN, "Insufficient scope"),
        };
        (status, message).into_response()
    }
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;
    use ed25519_dalek::SigningKey;
    use tpt_soma_capability::signing::LocalSigningBackend;

    pub fn signed_token(
        signing_key: &SigningKey,
        subject: &str,
        resource_class: &str,
        cohort_scope: Vec<String>,
        action: &str,
        nonce: [u8; 32],
    ) -> String {
        let backend = LocalSigningBackend::new(signing_key.clone());
        let token = CapabilityToken {
            subject: subject.to_string(),
            resource_class: resource_class.to_string(),
            cohort_scope,
            action: action.to_string(),
            expiry: u64::MAX,
            nonce: nonce.to_vec(),
            signature: Vec::new(),
            graph_scope: None,
        };
        let signed = CapabilityToken::sign(&backend, token);
        serde_json::to_string(&signed).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(resource_class: &str, cohort: &str, action: &str) -> CapabilityToken {
        CapabilityToken {
            subject: "researcher-1".to_string(),
            resource_class: resource_class.to_string(),
            cohort_scope: vec![cohort.to_string()],
            action: action.to_string(),
            expiry: u64::MAX,
            nonce: vec![1u8; 32],
            signature: Vec::new(),
            graph_scope: None,
        }
    }

    #[test]
    fn test_route_policy_allows_correct_class_action_cohort() {
        let t = token("genomic_variant", "cohort-a", "read");
        enforce_route_policy(&t, "GET", "/api/v1/variants/sample-1").unwrap();
        enforce_route_policy(&t, "GET", "/api/v1/cohorts/cohort-a/samples").unwrap();
    }

    #[test]
    fn test_route_policy_rejects_wrong_resource_class() {
        let t = token("clinical_observation", "cohort-a", "read");
        let err = enforce_route_policy(&t, "GET", "/api/v1/variants/sample-1").unwrap_err();
        assert!(matches!(err, AuthError::InsufficientScope));
    }

    #[test]
    fn test_route_policy_rejects_wrong_action() {
        let t = token("genomic_raw", "cohort-a", "read");
        let err = enforce_route_policy(&t, "POST", "/api/v1/ingest/vcf").unwrap_err();
        assert!(matches!(err, AuthError::InsufficientScope));
    }

    #[test]
    fn test_route_policy_write_implies_read() {
        let t = token("genomic_variant", "cohort-a", "write");
        enforce_route_policy(&t, "GET", "/api/v1/variants/sample-1").unwrap();
    }

    #[test]
    fn test_route_policy_rejects_wrong_cohort() {
        let t = token("genomic_variant", "cohort-a", "read");
        let err = enforce_route_policy(&t, "GET", "/api/v1/cohorts/cohort-b/samples").unwrap_err();
        assert!(matches!(err, AuthError::InsufficientScope));
    }

    #[test]
    fn test_route_policy_wildcard_cohort_allowed() {
        let t = token("genomic_variant", "*", "read");
        enforce_route_policy(&t, "GET", "/api/v1/cohorts/cohort-b/samples").unwrap();
    }

    #[test]
    fn test_route_policy_export_required_for_aggregate() {
        let read = token("genomic_variant", "cohort-a", "read");
        let err = enforce_route_policy(&read, "POST", "/api/v1/cohorts/cohort-a/aggregate/count")
            .unwrap_err();
        assert!(matches!(err, AuthError::InsufficientScope));

        let export = token("genomic_variant", "cohort-a", "export");
        enforce_route_policy(&export, "POST", "/api/v1/cohorts/cohort-a/aggregate/count").unwrap();
    }

    #[test]
    fn test_route_policy_unmatched_route_allowed() {
        let t = token("genomic_variant", "cohort-a", "read");
        enforce_route_policy(&t, "GET", "/health").unwrap();
    }

    #[test]
    fn test_path_matching() {
        assert!(path_matches("/api/v1/variants/s1", "/api/v1/variants/*"));
        assert!(path_matches(
            "/api/v1/variants/s1/v2",
            "/api/v1/variants/*/*"
        ));
        assert!(!path_matches("/api/v1/variants/s1", "/api/v1/variants/*/*"));
        assert!(path_matches(
            "/api/v1/clinical-observations/s1/LOINC-1/trajectory",
            "/api/v1/clinical-observations/*/*/trajectory"
        ));
    }

    #[test]
    fn test_extract_cohort_id() {
        assert_eq!(
            extract_cohort_id("/api/v1/cohorts/cohort-a/samples").as_deref(),
            Some("cohort-a")
        );
        assert_eq!(
            extract_cohort_id("/api/v1/cohorts/cohort-b/aggregate/count").as_deref(),
            Some("cohort-b")
        );
        assert_eq!(extract_cohort_id("/api/v1/variants/s1"), None);
    }

    #[test]
    fn test_graph_scope_allows_unscoped_token() {
        let t = token("genomic_variant", "cohort-a", "read");
        assert!(graph_scope_allows(&t, "any-node"));
    }

    #[test]
    fn test_graph_scope_allows_matching_entity() {
        let mut t = token("genomic_variant", "cohort-a", "read");
        t.graph_scope = Some(vec!["node-x".to_string()]);
        assert!(graph_scope_allows(&t, "node-x"));
        assert!(!graph_scope_allows(&t, "node-y"));
    }

    #[test]
    fn test_graph_scope_wildcard_allows_all() {
        let mut t = token("genomic_variant", "cohort-a", "read");
        t.graph_scope = Some(vec!["*".to_string()]);
        assert!(graph_scope_allows(&t, "node-anything"));
    }
}
