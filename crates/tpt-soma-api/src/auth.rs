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

/// Writes a `dp_budget` spend row. Callers await this before releasing a noisy
/// result, so a budget that cannot be recorded is never spent unseen.
pub async fn record_dp_budget_spend(
    audit_ledger: &AuditLedger,
    actor: &str,
    cohort: &str,
    epsilon_spent: f64,
) -> Result<(), tpt_soma_audit::ledger::AuditError> {
    let event = AuditEvent {
        id: Uuid::new_v4(),
        actor: actor.to_string(),
        resource_class: "dp_budget".to_string(),
        action: "spend".to_string(),
        cohort_scope: vec![cohort.to_string()],
        timestamp: Utc::now(),
        query_fingerprint: dp_spend_fingerprint(epsilon_spent),
        outcome: "success".to_string(),
        prev_row_hash: None,
        row_hash: String::new(),
    };
    audit_ledger.append(event).await
}

/// Fingerprint layout is read back by [`restore_dp_budget`]; change both together.
fn dp_spend_fingerprint(epsilon_spent: f64) -> String {
    format!("dp_budget_spend:epsilon={}", epsilon_spent)
}

/// Rebuilds each cohort's spent epsilon from the `dp_budget` rows. Without this,
/// a restart or a second replica starts every cohort with a full budget.
/// Returns the number of spend rows applied.
pub async fn restore_dp_budget(
    audit_ledger: &AuditLedger,
    dp: &mut tpt_soma_core::DifferentialPrivacyService,
) -> Result<usize, String> {
    let rows = sqlx::query_as::<_, (Option<String>, String)>(
        r#"
        SELECT cohort_scope[1], query_fingerprint
        FROM audit_ledger
        WHERE resource_class = 'dp_budget' AND action = 'spend' AND outcome = 'success'
        ORDER BY seq ASC
        "#,
    )
    .fetch_all(&audit_ledger.pool)
    .await
    .map_err(|e| format!("reading dp_budget rows: {e}"))?;

    let mut applied = 0;
    for (cohort, fingerprint) in rows {
        let cohort = cohort.ok_or("dp_budget row has no cohort")?;
        // Accepts the current layout and the older `cohort=…:epsilon=…` one.
        let epsilon = fingerprint
            .rsplit_once("epsilon=")
            .and_then(|(_, value)| value.parse::<f64>().ok())
            .ok_or_else(|| format!("unrecognised dp_budget fingerprint: {fingerprint}"))?;
        dp.restore_spend(&cohort, epsilon);
        applied += 1;
    }
    Ok(applied)
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
        "GET",
        "/api/v1/cohorts/*/budget",
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
    // Phase 4: systemic query computes a trajectory summary without persisting it
    (
        "POST",
        "/api/v1/systemic/query",
        &["simulation_output"],
        "read",
    ),
    // Phase 4: clinical trial protocols, screening and adverse events
    ("POST", "/api/v1/trials", &["clinical_trial"], "write"),
    (
        "POST",
        "/api/v1/trials/*/screen",
        &["clinical_trial"],
        "write",
    ),
    (
        "POST",
        "/api/v1/trials/*/adverse-events",
        &["clinical_trial"],
        "write",
    ),
    (
        "GET",
        "/api/v1/trials/*/adverse-events/summary",
        &["clinical_trial"],
        "read",
    ),
    (
        "GET",
        "/api/v1/trials/*/subjects/*/schedule",
        &["clinical_trial"],
        "read",
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
    // Trial ids are cohort scopes too: a token scoped to one trial cannot reach another.
    let rest = path
        .strip_prefix("/api/v1/cohorts/")
        .or_else(|| path.strip_prefix("/api/v1/trials/"))?;
    let id = rest.split('/').next().unwrap_or("");
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// One check made by the route policy, with its outcome and the values it compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyCheck {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

/// Evaluate every route-policy check for a token and report each one. The
/// enforcing path (`enforce_route_policy`) is built on this, so an explanation
/// cannot drift from what the server actually does.
///
/// This covers policy only. It does not check signature, expiry or revocation.
pub fn explain_route_policy(token: &CapabilityToken, method: &str, path: &str) -> Vec<PolicyCheck> {
    // Default deny: a route without a POLICY entry is refused for every token.
    // Adding a route means adding its policy entry; the route-coverage test in
    // `server.rs` fails until that is done.
    let Some(required) = required_capability_for(method, path) else {
        return vec![PolicyCheck {
            name: "policy entry",
            passed: false,
            detail: format!("no POLICY entry for {method} {path} (default deny)"),
        }];
    };

    let mut checks = vec![
        PolicyCheck {
            name: "resource class",
            passed: required
                .allowed_classes
                .contains(&token.resource_class.as_str()),
            detail: format!(
                "token '{}' against allowed {:?}",
                token.resource_class, required.allowed_classes
            ),
        },
        PolicyCheck {
            name: "action",
            passed: action_allows(&token.action, required.action),
            detail: format!(
                "token '{}' against required '{}'",
                token.action, required.action
            ),
        },
    ];

    // Creating a trial mints a new cohort id, so only an unscoped (`*`) token may do it.
    if path == "/api/v1/trials" {
        checks.push(PolicyCheck {
            name: "trial creation",
            passed: token.cohort_scope.iter().any(|c| c == "*"),
            detail: format!("requires '*' scope; token has {:?}", token.cohort_scope),
        });
    }

    if let Some(cohort_id) = extract_cohort_id(path) {
        checks.push(PolicyCheck {
            name: "cohort scope",
            passed: token
                .cohort_scope
                .iter()
                .any(|c| c == "*" || c == &cohort_id),
            detail: format!(
                "cohort '{}' against scope {:?}",
                cohort_id, token.cohort_scope
            ),
        });
    }

    checks
}

/// Enforce the token's resource class, action, and cohort scope against the
/// route-level policy. Returns Ok(()) when the token satisfies the policy.
pub fn enforce_route_policy(
    token: &CapabilityToken,
    method: &str,
    path: &str,
) -> Result<(), AuthError> {
    if explain_route_policy(token, method, path)
        .iter()
        .all(|check| check.passed)
    {
        Ok(())
    } else {
        Err(AuthError::InsufficientScope)
    }
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

    // Fail closed: the response is released only once its audit row is durable.
    // If the ledger is unavailable, the caller gets no payload.
    if let Err(err) = state.audit_ledger.append(audit_event).await {
        tracing::error!(error = %err, "audit append failed; withholding response");
        return Err(AuthError::AuditUnavailable);
    }

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
    #[error("audit ledger unavailable")]
    AuditUnavailable,
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
            AuthError::AuditUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Audit ledger unavailable; response withheld",
            ),
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
    fn test_explain_reports_each_failing_check() {
        // Right class and action, wrong cohort: only the cohort check fails.
        let t = token("genomic_variant", "cohort-b", "read");
        let checks = explain_route_policy(&t, "GET", "/api/v1/cohorts/cohort-a/samples");
        let failed: Vec<_> = checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.name)
            .collect();
        assert_eq!(failed, vec!["cohort scope"]);
        assert!(enforce_route_policy(&t, "GET", "/api/v1/cohorts/cohort-a/samples").is_err());
    }

    #[test]
    fn test_explain_reports_default_deny_for_unlisted_route() {
        let t = token("genomic_variant", "*", "admin");
        let checks = explain_route_policy(&t, "DELETE", "/api/v1/nowhere");
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].name, "policy entry");
        assert!(!checks[0].passed);
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
    fn test_route_policy_systemic_query_requires_simulation_output() {
        let sim = token("simulation_output", "cohort-a", "read");
        enforce_route_policy(&sim, "POST", "/api/v1/systemic/query").unwrap();

        let wrong_class = token("genomic_variant", "cohort-a", "read");
        assert!(matches!(
            enforce_route_policy(&wrong_class, "POST", "/api/v1/systemic/query").unwrap_err(),
            AuthError::InsufficientScope
        ));

        // An export token outranks read, so it is accepted too.
        let export = token("simulation_output", "cohort-a", "export");
        enforce_route_policy(&export, "POST", "/api/v1/systemic/query").unwrap();
    }

    #[test]
    fn test_route_policy_trials_class_action_and_scope() {
        let wrong = token("genomic_variant", "trial-1", "write");
        assert!(matches!(
            enforce_route_policy(&wrong, "POST", "/api/v1/trials/trial-1/screen").unwrap_err(),
            AuthError::InsufficientScope
        ));

        let read = token("clinical_trial", "trial-1", "read");
        enforce_route_policy(
            &read,
            "GET",
            "/api/v1/trials/trial-1/adverse-events/summary",
        )
        .unwrap();
        assert!(matches!(
            enforce_route_policy(&read, "POST", "/api/v1/trials/trial-1/adverse-events")
                .unwrap_err(),
            AuthError::InsufficientScope
        ));

        // Scoped to trial-1 only: trial-2 is out of scope.
        let write = token("clinical_trial", "trial-1", "write");
        enforce_route_policy(&write, "POST", "/api/v1/trials/trial-1/screen").unwrap();
        assert!(matches!(
            enforce_route_policy(&write, "POST", "/api/v1/trials/trial-2/screen").unwrap_err(),
            AuthError::InsufficientScope
        ));
    }

    #[test]
    fn test_route_policy_subject_schedule_is_read_and_trial_scoped() {
        let path = "/api/v1/trials/trial-1/subjects/subject-7/schedule";
        let read = token("clinical_trial", "trial-1", "read");
        enforce_route_policy(&read, "GET", path).unwrap();

        // Read-only: a write action is not enough to see a schedule and a read is not enough to write one.
        let write = token("clinical_trial", "trial-1", "write");
        enforce_route_policy(&write, "GET", path).unwrap();
        assert!(matches!(
            enforce_route_policy(&read, "POST", path).unwrap_err(),
            AuthError::InsufficientScope
        ));

        // A token scoped to another trial cannot read this trial's schedule.
        let other = token("clinical_trial", "trial-2", "read");
        assert!(matches!(
            enforce_route_policy(&other, "GET", path).unwrap_err(),
            AuthError::InsufficientScope
        ));
    }

    #[test]
    fn test_route_policy_trial_creation_requires_wildcard_scope() {
        let scoped = token("clinical_trial", "trial-1", "write");
        assert!(matches!(
            enforce_route_policy(&scoped, "POST", "/api/v1/trials").unwrap_err(),
            AuthError::InsufficientScope
        ));
        let wildcard = token("clinical_trial", "*", "write");
        enforce_route_policy(&wildcard, "POST", "/api/v1/trials").unwrap();
    }

    #[test]
    fn test_route_policy_unmatched_route_denied() {
        // Default deny: a route with no POLICY entry is refused for every token.
        let t = token("genomic_variant", "cohort-a", "admin");
        assert!(enforce_route_policy(&t, "GET", "/api/v1/not-a-real-route").is_err());
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

#[cfg(test)]
mod db_tests {
    use super::*;

    /// A fresh service stands in for a restarted process. The spends must still
    /// count against the cohort's budget.
    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_dp_spend_survives_restart() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    {
        let pool = tpt_soma_core::test_helpers::test_pool().await?;
        let ledger = AuditLedger::new(pool.clone());
        sqlx::query("DELETE FROM audit_ledger")
            .execute(&pool)
            .await?;

        record_dp_budget_spend(&ledger, "researcher-1", "cohort-restart", 0.4).await?;
        record_dp_budget_spend(&ledger, "researcher-1", "cohort-restart", 0.3).await?;

        let mut restarted = tpt_soma_core::DifferentialPrivacyService::new(1.0);
        let applied = restore_dp_budget(&ledger, &mut restarted).await?;
        assert_eq!(applied, 2);
        assert!((restarted.remaining_budget("cohort-restart") - 0.3).abs() < 1e-9);
        Ok(())
    }
}
