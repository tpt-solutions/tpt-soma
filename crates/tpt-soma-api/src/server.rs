use std::net::SocketAddr;
use axum::{
    routing::{get, post},
    Router,
    middleware,
};
use crate::auth::{AuthState, capability_middleware};
use crate::error::{ApiError, Result};
use tpt_soma_core::connection::PgPool;
use std::sync::Arc;
use ed25519_dalek::VerifyingKey;
use tpt_soma_capability::RevocationList;
use tpt_soma_audit::AuditLedger;
use tpt_soma_ingest::endpoint;

pub struct ApiServer {
    pub addr: SocketAddr,
    pub pool: PgPool,
    pub verifying_key: VerifyingKey,
    pub revocation_list: Arc<RevocationList>,
    pub audit_ledger: Arc<AuditLedger>,
}

impl ApiServer {
    pub async fn run(self) -> Result<()> {
        let auth_state = Arc::new(AuthState {
            pool: self.pool.clone(),
            verifying_key: self.verifying_key,
            revocation_list: self.revocation_list,
            audit_ledger: self.audit_ledger,
        });

        let app = Router::new()
            .route("/health", get(health_check))
            // Genomica routes
            .route("/api/v1/variants/:sample_id", get(get_variants))
            .route("/api/v1/variants/:sample_id/:variant_id", get(get_variant))
            // Cytos routes
            .route("/api/v1/expression/:sample_id", get(get_expression))
            .route("/api/v1/expression/gene/:gene_id", get(get_expression_by_gene))
            .route("/api/v1/umap/:sample_id", get(get_umap))
            .route("/api/v1/umap/:sample_id/cluster/:cluster", get(get_umap_by_cluster))
            // Multi-omics join
            .route("/api/v1/join/variant-expression", post(join_variant_expression))
            // Cohort routes
            .route("/api/v1/cohorts/:cohort_id/samples", get(get_cohort_samples))
            // Ingest routes (no auth required for upload)
            .route("/api/v1/ingest/vcf", post(upload_vcf))
            .route("/api/v1/ingest/h5ad", post(upload_h5ad))
            .layer(middleware::from_fn_with_state(auth_state.clone(), capability_middleware))
            .with_state(auth_state);

        let listener = tokio::net::TcpListener::bind(self.addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn health_check() -> &'static str {
    "ok"
}

// Ingest handlers (re-exported from tpt-soma-ingest)
async fn upload_vcf(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    mut multipart: axum::extract::Multipart,
) -> Result<impl axum::response::IntoResponse> {
    let mut tmp_path = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError::BadRequest(e.to_string()))? {
        let name = field.name().unwrap_or("");
        if name == "file" {
            let data = field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
            let path = format!("/tmp/upload_{}.vcf", uuid::Uuid::new_v4());
            std::fs::write(&path, data)?;
            tmp_path = Some(path);
        }
    }
    let path = tmp_path.ok_or_else(|| ApiError::BadRequest("missing file field".into()))?;
    let records = tpt_soma_ingest::vcf::VcfParser::new(&path).parse().map_err(|e| ApiError::BadRequest(e.to_string()))?;
    std::fs::remove_file(&path)?;
    Ok(axum::Json(serde_json::json!({"status": "ok", "records": records.len()})))
}

async fn upload_h5ad(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    mut multipart: axum::extract::Multipart,
) -> Result<impl axum::response::IntoResponse> {
    let mut tmp_path = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError::BadRequest(e.to_string()))? {
        let name = field.name().unwrap_or("");
        if name == "file" {
            let data = field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?;
            let path = format!("/tmp/upload_{}.h5ad", uuid::Uuid::new_v4());
            std::fs::write(&path, data)?;
            tmp_path = Some(path);
        }
    }
    let path = tmp_path.ok_or_else(|| ApiError::BadRequest("missing file field".into()))?;
    let result = tpt_soma_ingest::h5ad::AnnDataParser::new(&path).parse().map_err(|e| ApiError::BadRequest(e.to_string()))?;
    std::fs::remove_file(&path)?;
    Ok(axum::Json(serde_json::json!({"status": "ok", "cells": result.n_cells()})))
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
    let records = tpt_soma_core::query::get_umap_by_cluster(&state.pool, &sample_id, &cluster).await?;
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
    ).await?;
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