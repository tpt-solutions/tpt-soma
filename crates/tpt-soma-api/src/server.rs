use crate::auth::{AuthState, capability_middleware};
use crate::error::{ApiError, Result};
use axum::{
    Router, middleware,
    response::IntoResponse,
    routing::{get, post},
};
use ed25519_dalek::VerifyingKey;
use prometheus::Encoder;
use std::net::SocketAddr;
use std::sync::Arc;
use tpt_soma_audit::AuditLedger;
use tpt_soma_capability::RevocationList;
use tpt_soma_core::DifferentialPrivacyService;
use tpt_soma_core::connection::PgPool;
use tpt_soma_core::store::ObjectStoreClient;
use tpt_soma_ingest::endpoint;

pub struct ApiServer {
    pub addr: SocketAddr,
    pub pool: PgPool,
    pub verifying_key: VerifyingKey,
    pub revocation_list: Arc<RevocationList>,
    pub audit_ledger: Arc<AuditLedger>,
    pub object_store: Arc<ObjectStoreClient>,
    pub dp_epsilon: f64,
}

impl ApiServer {
    pub async fn run(self) -> Result<()> {
        let dp_service = Arc::new(tokio::sync::Mutex::new(
            DifferentialPrivacyService::new(self.dp_epsilon).with_audit_hook(Arc::new({
                let audit_ledger = self.audit_ledger.clone();
                move |cohort, epsilon_spent, actor| {
                    let ledger = audit_ledger.clone();
                    let actor = actor.clone();
                    tokio::spawn(async move {
                        let _ = crate::auth::record_dp_budget_spend(
                            &ledger,
                            &actor,
                            &cohort,
                            epsilon_spent,
                        )
                        .await;
                    });
                }
            })),
        ));

        let auth_state = Arc::new(AuthState {
            pool: self.pool.clone(),
            verifying_key: self.verifying_key,
            revocation_list: self.revocation_list,
            audit_ledger: self.audit_ledger,
            dp_service,
        });

        let ingest_router = endpoint::router()
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                capability_middleware,
            ))
            .with_state(self.object_store.clone());

        let app = Router::new()
            .route("/health", get(health_check))
            .route("/metrics", get(metrics_handler))
            // Ingest routes
            .nest("/api/v1", ingest_router)
            // Genomica routes
            .route("/api/v1/variants/:sample_id", get(get_variants))
            .route("/api/v1/variants/:sample_id/:variant_id", get(get_variant))
            // Cytos routes
            .route("/api/v1/expression/:sample_id", get(get_expression))
            .route(
                "/api/v1/expression/gene/:gene_id",
                get(get_expression_by_gene),
            )
            .route("/api/v1/umap/:sample_id", get(get_umap))
            .route(
                "/api/v1/umap/:sample_id/cluster/:cluster",
                get(get_umap_by_cluster),
            )
            // Multi-omics join
            .route(
                "/api/v1/join/variant-expression",
                post(join_variant_expression),
            )
            // Cohort routes
            .route(
                "/api/v1/cohorts/:cohort_id/samples",
                get(get_cohort_samples),
            )
            // DP cohort aggregate export (single enforcement code path)
            .route(
                "/api/v1/cohorts/:cohort_id/aggregate/count",
                post(cohort_aggregate_count),
            )
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                capability_middleware,
            ))
            .with_state(auth_state);

        let listener = tokio::net::TcpListener::bind(self.addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn health_check() -> &'static str {
    "ok"
}

// Genomica handlers
#[axum::debug_handler]
async fn get_variants(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(sample_id): axum::extract::Path<String>,
) -> Result<axum::Json<Vec<tpt_soma_core::query::VariantRecord>>> {
    let records = tpt_soma_core::query::get_variants_by_sample(&state.pool, &sample_id).await?;
    Ok(axum::Json(records))
}

#[axum::debug_handler]
async fn get_variant(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path((sample_id, variant_id)): axum::extract::Path<(String, String)>,
) -> Result<axum::Json<Option<tpt_soma_core::query::VariantRecord>>> {
    let records = tpt_soma_core::query::get_variants_by_sample(&state.pool, &sample_id).await?;
    let record = records.into_iter().find(|r| r.variant_id == variant_id);
    Ok(axum::Json(record))
}

// Cytos handlers
#[axum::debug_handler]
async fn get_expression(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(sample_id): axum::extract::Path<String>,
) -> Result<axum::Json<Vec<tpt_soma_core::query::ExpressionRecord>>> {
    let records = tpt_soma_core::query::get_expression_by_sample(&state.pool, &sample_id).await?;
    Ok(axum::Json(records))
}

#[axum::debug_handler]
async fn get_expression_by_gene(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(gene_id): axum::extract::Path<String>,
) -> Result<axum::Json<Vec<tpt_soma_core::query::ExpressionRecord>>> {
    let records = tpt_soma_core::query::get_expression_by_gene(&state.pool, &gene_id).await?;
    Ok(axum::Json(records))
}

#[axum::debug_handler]
async fn get_umap(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(sample_id): axum::extract::Path<String>,
) -> Result<axum::Json<Vec<tpt_soma_core::query::UmapRecord>>> {
    let records = tpt_soma_core::query::get_umap_by_sample(&state.pool, &sample_id).await?;
    Ok(axum::Json(records))
}

#[axum::debug_handler]
async fn get_umap_by_cluster(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path((sample_id, cluster)): axum::extract::Path<(String, String)>,
) -> Result<axum::Json<Vec<tpt_soma_core::query::UmapRecord>>> {
    let records =
        tpt_soma_core::query::get_umap_by_cluster(&state.pool, &sample_id, &cluster).await?;
    Ok(axum::Json(records))
}

// Multi-omics join
#[axum::debug_handler]
async fn join_variant_expression(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Json(payload): axum::extract::Json<JoinRequest>,
) -> Result<axum::Json<Vec<tpt_soma_core::query::VariantExpressionJoin>>> {
    let records = tpt_soma_core::query::join_variant_expression(
        &state.pool,
        &payload.sample_id,
        &payload.variant_id,
        &payload.gene_id,
    )
    .await?;
    Ok(axum::Json(records))
}

#[derive(serde::Deserialize)]
struct JoinRequest {
    sample_id: String,
    variant_id: String,
    gene_id: String,
}

// Cohort handlers
#[axum::debug_handler]
async fn get_cohort_samples(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(cohort_id): axum::extract::Path<String>,
) -> Result<axum::Json<Vec<tpt_soma_core::query::SampleRecord>>> {
    let records = tpt_soma_core::query::get_cohort_samples(&state.pool, &cohort_id).await?;
    Ok(axum::Json(records))
}

#[derive(serde::Deserialize)]
struct AggregateCountRequest {
    cohort_id: String,
    sensitivity: Option<f64>,
}

#[axum::debug_handler]
async fn cohort_aggregate_count(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Json(payload): axum::extract::Json<AggregateCountRequest>,
) -> Result<axum::Json<serde_json::Value>> {
    let sensitivity = payload.sensitivity.unwrap_or(1.0);
    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cohort_membership WHERE cohort_id = $1")
            .bind(&payload.cohort_id)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut dp = state.dp_service.lock().await;
    let noisy_count =
        dp.cohort_aggregate_export(&payload.cohort_id, "api", count as usize, sensitivity)?;

    Ok(axum::Json(serde_json::json!({
        "cohort_id": payload.cohort_id,
        "true_count": count,
        "noisy_count": noisy_count,
        "epsilon_spent": sensitivity,
    })))
}

async fn metrics_handler() -> impl IntoResponse {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    if encoder.encode(&metric_families, &mut buffer).is_ok() {
        axum::http::Response::builder()
            .header("Content-Type", "text/plain; version=0.0.4")
            .body(axum::body::Body::from(buffer))
            .unwrap()
    } else {
        axum::http::Response::builder()
            .status(500)
            .body(axum::body::Body::from("failed to encode metrics"))
            .unwrap()
    }
}
