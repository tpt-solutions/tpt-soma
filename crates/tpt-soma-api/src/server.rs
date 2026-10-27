use crate::auth::{AuthState, capability_middleware, record_dp_budget_spend, restore_dp_budget};
use crate::error::{ApiError, Result};
use crate::rate_limit::{UPLOAD_BURST, UPLOAD_WINDOW, UploadRateLimiter, upload_rate_limit};
use axum::extract::DefaultBodyLimit;

/// Largest accepted request body. axum 0.8 puts no limit on multipart uploads by
/// default, so without this cap one client could stream an unbounded body into
/// memory. The cap is set above the largest expected VCF or h5ad file.
const MAX_REQUEST_BODY_BYTES: usize = 256 * 1024 * 1024;

fn apply_body_limit<S: Clone + Send + Sync + 'static>(
    router: Router<S>,
    max_bytes: usize,
) -> Router<S> {
    router.layer(DefaultBodyLimit::max(max_bytes))
}

/// Spends epsilon, records the spend in the ledger, and only then returns a
/// noisy count. The ledger write is awaited before anything is released, so a
/// spend that cannot be recorded never produces output. Holding the lock across
/// the write also stops concurrent exports from overspending a cohort's budget.
async fn release_dp_count(
    state: &AuthState,
    cohort: &str,
    actor: &str,
    count: usize,
    sensitivity: f64,
) -> Result<f64> {
    let mut dp = state.dp_service.lock().await;
    dp.spend_budget(cohort, actor, sensitivity)?;
    record_dp_budget_spend(&state.audit_ledger, actor, cohort, sensitivity)
        .await
        .map_err(|e| {
            ApiError::Internal(format!("audit ledger unavailable; export withheld: {e}"))
        })?;
    Ok(dp.noisy_count(count, sensitivity))
}
use axum::{
    Router, middleware,
    response::IntoResponse,
    routing::{get, post},
};
use ed25519_dalek::VerifyingKey;
use std::net::SocketAddr;
use std::sync::Arc;
use tpt_soma_audit::AuditLedger;
use tpt_soma_capability::RevocationList;
use tpt_soma_core::DifferentialPrivacyService;
use tpt_soma_core::connection::PgPool;
use tpt_soma_core::store::ObjectStoreClient;
use tpt_soma_ingest::endpoint;
use tpt_soma_simulacrum::{crosstalk, storage as sim_storage};
use uuid::Uuid;

use axum::extract::{Multipart, Query};
use tpt_soma_chronos::{
    cgm as chronos_cgm, storage as chronos_storage, trajectory as chronos_trajectory,
    variability as chronos_variability,
};
use tpt_soma_clinica::{
    ClinicaError,
    protocol::{Arm, ProtocolDesign, Visit, visit_schedule},
    storage as clinica_storage,
    trial::{AdverseEvent, Criterion, SubjectFeatures},
};
use tpt_soma_organon::{
    imaging as organon_imaging, ingestion as organon_ingestion, organ_system::OrganSystemGraph,
    storage as organon_storage,
};

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
        let mut dp = DifferentialPrivacyService::new(self.dp_epsilon);
        let restored = restore_dp_budget(&self.audit_ledger, &mut dp)
            .await
            .map_err(ApiError::Internal)?;
        tracing::info!(
            restored,
            "restored differential-privacy spend from audit ledger"
        );
        let dp_service = Arc::new(tokio::sync::Mutex::new(dp));

        let auth_state = Arc::new(AuthState {
            pool: self.pool.clone(),
            verifying_key: self.verifying_key,
            revocation_list: self.revocation_list,
            audit_ledger: self.audit_ledger,
            dp_service,
            object_store: self.object_store.clone(),
        });

        // Every multipart upload shares one per-subject rate limit. The limiter
        // sits inside the capability layer, so it sees the authenticated subject.
        let upload_limiter = Arc::new(UploadRateLimiter::new(UPLOAD_BURST, UPLOAD_WINDOW));
        let uploads = Router::new()
            .route("/api/v1/ingest/organ-csv", post(ingest_organ_csv))
            .route("/api/v1/ingest/imaging", post(ingest_organ_imaging))
            .route("/api/v1/ingest/cgm", post(ingest_cgm))
            .with_state(auth_state.clone())
            .nest(
                "/api/v1",
                endpoint::router().with_state(self.object_store.clone()),
            )
            .layer(middleware::from_fn_with_state(
                upload_limiter,
                upload_rate_limit,
            ))
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                capability_middleware,
            ));

        let app = Router::new()
            // Genomica routes
            .route("/api/v1/variants/{sample_id}", get(get_variants))
            .route(
                "/api/v1/variants/{sample_id}/{variant_id}",
                get(get_variant),
            )
            // Cytos routes
            .route("/api/v1/expression/{sample_id}", get(get_expression))
            .route(
                "/api/v1/expression/gene/{gene_id}",
                get(get_expression_by_gene),
            )
            .route("/api/v1/umap/{sample_id}", get(get_umap))
            .route(
                "/api/v1/umap/{sample_id}/cluster/{cluster}",
                get(get_umap_by_cluster),
            )
            // Multi-omics join
            .route(
                "/api/v1/join/variant-expression",
                post(join_variant_expression),
            )
            // Cohort routes
            .route(
                "/api/v1/cohorts/{cohort_id}/samples",
                get(get_cohort_samples),
            )
            // DP cohort aggregate export (single enforcement code path)
            .route(
                "/api/v1/cohorts/{cohort_id}/aggregate/count",
                post(cohort_aggregate_count),
            )
            // DP budget dry run: reports whether an export would fit, spends nothing
            .route("/api/v1/cohorts/{cohort_id}/budget", get(get_cohort_budget))
            // Organon: clinical observation ingestion + trajectories
            .route(
                "/api/v1/ingest/fhir-observation",
                post(ingest_fhir_observation),
            )
            .route(
                "/api/v1/cohorts/{cohort_id}/aggregate/cross-domain",
                post(cross_domain_aggregate_count),
            )
            .route(
                "/api/v1/clinical-observations/{subject_id}",
                get(get_clinical_observations),
            )
            .route(
                "/api/v1/clinical-observations/{subject_id}/{loinc_code}/trajectory",
                get(get_organ_trajectory),
            )
            // Organon: imaging ingestion + listing
            .route("/api/v1/organ-imaging/{subject_id}", get(get_organ_imaging))
            // Organon: cross-organ coupling graph
            .route("/api/v1/organ-system-graph", get(get_organ_system_graph))
            .route(
                "/api/v1/organ-system-graph/cascade",
                post(simulate_organ_cascade),
            )
            // Chronos: CGM ingestion + variability
            .route("/api/v1/cgm/{subject_id}", get(get_cgm_readings))
            .route(
                "/api/v1/cgm/{subject_id}/variability",
                get(get_cgm_variability),
            )
            // Cross-phase: Phase 1 samples + Phase 2 clinical/CGM records for one subject
            .route(
                "/api/v1/subjects/{patient_id}/cross-phase-summary",
                get(get_cross_phase_summary),
            )
            // Phase 3: simulation run + query + DP aggregate export
            .route("/api/v1/simulate", post(run_simulation))
            // Phase 4: systemic query over an OSG topology (computed, not persisted)
            .route("/api/v1/systemic/query", post(run_systemic_query))
            // Phase 4: clinical trial protocols, screening and adverse events
            .route("/api/v1/trials", post(create_trial))
            .route("/api/v1/trials/{trial_id}/screen", post(screen_trial))
            .route(
                "/api/v1/trials/{trial_id}/adverse-events",
                post(record_trial_adverse_events),
            )
            .route(
                "/api/v1/trials/{trial_id}/adverse-events/summary",
                get(get_trial_ae_summary),
            )
            .route(
                "/api/v1/trials/{trial_id}/subjects/{subject_id}/schedule",
                get(get_subject_schedule),
            )
            .route("/api/v1/simulations/{run_id}", get(get_simulation))
            .route(
                "/api/v1/simulations/{run_id}/aggregate/count",
                post(simulation_aggregate_count),
            )
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                capability_middleware,
            ))
            .with_state(auth_state);

        // Probes and metrics sit outside the capability layer, so orchestrators
        // can reach them without a token.
        let public = Router::new()
            .route("/health", get(health_check))
            .route("/metrics", get(metrics_handler));
        let app = apply_body_limit(app.merge(uploads).merge(public), MAX_REQUEST_BODY_BYTES);

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

    let noisy_count = release_dp_count(
        &state,
        &payload.cohort_id,
        "api",
        count as usize,
        sensitivity,
    )
    .await?;

    // NOTE: only the noisy count leaves the process. The true count is never
    // returned, otherwise differential privacy would be defeated.
    Ok(axum::Json(serde_json::json!({
        "cohort_id": payload.cohort_id,
        "noisy_count": noisy_count,
        "epsilon_spent": sensitivity,
    })))
}

#[derive(serde::Deserialize)]
struct BudgetDryRunQuery {
    sensitivity: Option<f64>,
}

#[axum::debug_handler]
async fn get_cohort_budget(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(cohort_id): axum::extract::Path<String>,
    Query(params): Query<BudgetDryRunQuery>,
) -> Result<axum::Json<serde_json::Value>> {
    let sensitivity = params.sensitivity.unwrap_or(1.0);
    if !(sensitivity.is_finite() && sensitivity > 0.0) {
        return Err(ApiError::BadRequest(
            "sensitivity must be a positive, finite number".to_string(),
        ));
    }

    let dp = state.dp_service.lock().await;
    Ok(axum::Json(serde_json::json!({
        "cohort_id": cohort_id,
        "epsilon_cap": dp.epsilon_cap(),
        "remaining_budget": dp.remaining_budget(&cohort_id),
        "requested_sensitivity": sensitivity,
        "would_succeed": dp.can_spend(&cohort_id, sensitivity),
    })))
}

// Organon handlers: clinical observation ingestion + trajectories

#[axum::debug_handler]
async fn ingest_fhir_observation(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Json(payload): axum::extract::Json<organon_ingestion::FhirObservation>,
) -> Result<axum::Json<serde_json::Value>> {
    let observation = organon_ingestion::parse_fhir_observation(&payload)?;
    let raw_payload =
        serde_json::to_value(&payload).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    organon_storage::insert_clinical_observation(&state.pool, &observation, "fhir").await?;
    organon_storage::insert_fhir_resource_payload(
        &state.pool,
        "Observation",
        &payload.id,
        &raw_payload,
    )
    .await?;

    Ok(axum::Json(serde_json::json!({
        "status": "ok",
        "observation_id": observation.id,
    })))
}

#[axum::debug_handler]
async fn ingest_organ_csv(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    mut multipart: Multipart,
) -> Result<axum::Json<serde_json::Value>> {
    let mut content = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
    {
        if field.name().unwrap_or("") == "file" {
            content = Some(
                field
                    .text()
                    .await
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?,
            );
        }
    }
    let content = content.ok_or_else(|| ApiError::BadRequest("missing field: file".to_string()))?;

    let records = match organon_ingestion::parse_organ_function_csv(&content) {
        Ok(records) => records,
        Err(e) => {
            let quarantine_key = format!("quarantine/organ-csv/{}.csv", uuid::Uuid::new_v4());
            let _ = state
                .object_store
                .upload_to_quarantine(&quarantine_key, content.into_bytes())
                .await;
            return Err(ApiError::BadRequest(format!(
                "{} (quarantined as {})",
                e, quarantine_key
            )));
        }
    };

    let mut observations = Vec::with_capacity(records.len());
    for record in records {
        observations.push(organon_ingestion::csv_to_clinical_observation(record)?);
    }
    let count =
        organon_storage::insert_clinical_observations(&state.pool, &observations, "csv").await?;

    Ok(axum::Json(serde_json::json!({
        "status": "ok",
        "observations_ingested": count,
    })))
}

#[axum::debug_handler]
async fn get_clinical_observations(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(subject_id): axum::extract::Path<String>,
) -> Result<axum::Json<Vec<tpt_soma_core::query::ClinicalObservationRecord>>> {
    let records =
        tpt_soma_core::query::get_clinical_observations_by_subject(&state.pool, &subject_id)
            .await?;
    Ok(axum::Json(records))
}

#[axum::debug_handler]
async fn get_organ_trajectory(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path((subject_id, loinc_code)): axum::extract::Path<(String, String)>,
) -> Result<axum::Json<chronos_trajectory::TrajectoryAnalysis>> {
    let records = tpt_soma_core::query::get_clinical_observations_by_subject_and_loinc(
        &state.pool,
        &subject_id,
        &loinc_code,
    )
    .await?;

    if records.is_empty() {
        return Err(ApiError::NotFound(format!(
            "no observations for subject {} loinc {}",
            subject_id, loinc_code
        )));
    }

    let test_name = records[0].loinc_code.clone();
    let unit = records[0].unit.clone().unwrap_or_default();
    let trajectory = chronos_trajectory::OrganFunctionTrajectory {
        id: uuid::Uuid::new_v4(),
        subject_id: subject_id.clone(),
        test_loinc: loinc_code.clone(),
        test_name,
        unit,
        time_points: records
            .iter()
            .map(|r| chronos_trajectory::TrajectoryPoint {
                timestamp: r.effective_time,
                value: r.value,
                is_interpolated: false,
                visit_id: None,
            })
            .collect(),
        reference_range: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let analysis = chronos_trajectory::analyze_trajectory(&trajectory)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(axum::Json(analysis))
}

// Organon handlers: organ imaging

#[axum::debug_handler]
async fn ingest_organ_imaging(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    mut multipart: Multipart,
) -> Result<axum::Json<serde_json::Value>> {
    let mut metadata: Option<ImagingIngestMetadata> = None;
    let mut data: Option<bytes::Bytes> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
    {
        match field.name().unwrap_or("") {
            "metadata" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
                metadata = Some(
                    serde_json::from_str(&text).map_err(|e| ApiError::BadRequest(e.to_string()))?,
                );
            }
            "file" => {
                data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::BadRequest(e.to_string()))?,
                );
            }
            _ => {}
        }
    }

    let metadata =
        metadata.ok_or_else(|| ApiError::BadRequest("missing field: metadata".to_string()))?;
    let data = data.ok_or_else(|| ApiError::BadRequest("missing field: file".to_string()))?;

    let object_key = organon_imaging::generate_imaging_object_key(
        &metadata.subject_id,
        &metadata.study_instance_uid,
        &metadata.series_instance_uid,
        &metadata.sop_instance_uid,
    );
    let checksum = format!("{:x}", md5::compute(&data));
    state
        .object_store
        .upload_with_checksum(&object_key, data.to_vec(), &checksum)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let record =
        organon_imaging::OrganImagingRecord {
            id: uuid::Uuid::new_v4(),
            subject_id: metadata.subject_id.clone(),
            dicom_metadata: organon_imaging::DicomMetadata {
                study_instance_uid: metadata.study_instance_uid,
                series_instance_uid: metadata.series_instance_uid,
                sop_instance_uid: metadata.sop_instance_uid,
                patient_id: metadata.subject_id.clone(),
                patient_name: None,
                study_date: None,
                study_time: None,
                modality: metadata.modality.parse().unwrap_or(
                    organon_imaging::ImagingModality::Other("unknown".to_string()),
                ),
                body_part_examined: metadata.body_part_examined,
                rows: metadata.rows.unwrap_or(0),
                columns: metadata.columns.unwrap_or(0),
                bits_allocated: 16,
                bits_stored: 16,
                pixel_spacing: None,
                slice_thickness: None,
                manufacturer: None,
                manufacturer_model: None,
                institution_name: None,
                series_description: None,
                protocol_name: None,
            },
            minio_bucket: state.object_store.bucket().to_string(),
            minio_object_key: object_key.clone(),
            checksum_sha256: checksum,
            file_size_bytes: data.len() as u64,
            ingested_at: chrono::Utc::now(),
            organ_system: metadata.organ_system,
            laterality: None,
            view_position: None,
        };

    organon_storage::insert_organ_imaging_record(&state.pool, &record).await?;

    Ok(axum::Json(serde_json::json!({
        "status": "ok",
        "object_key": object_key,
    })))
}

#[derive(serde::Deserialize)]
struct ImagingIngestMetadata {
    subject_id: String,
    study_instance_uid: String,
    series_instance_uid: String,
    sop_instance_uid: String,
    modality: String,
    body_part_examined: Option<String>,
    organ_system: Option<String>,
    rows: Option<u32>,
    columns: Option<u32>,
}

#[axum::debug_handler]
async fn get_organ_imaging(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(subject_id): axum::extract::Path<String>,
) -> Result<axum::Json<Vec<tpt_soma_core::query::OrganImagingRecord>>> {
    let records =
        tpt_soma_core::query::get_organ_imaging_by_subject(&state.pool, &subject_id).await?;
    Ok(axum::Json(records))
}

// Organon handlers: cross-organ coupling graph

#[axum::debug_handler]
async fn get_organ_system_graph() -> axum::Json<OrganSystemGraph> {
    axum::Json(OrganSystemGraph::default())
}

#[derive(serde::Deserialize)]
struct CascadeRequest {
    initial_organ: String,
    initial_dysfunction: String,
    severity: f64,
}

#[axum::debug_handler]
async fn simulate_organ_cascade(
    axum::extract::Json(payload): axum::extract::Json<CascadeRequest>,
) -> axum::Json<tpt_soma_organon::organ_system::DysfunctionCascade> {
    let graph = OrganSystemGraph::default();
    let cascade = graph.simulate_cascade(
        &payload.initial_organ,
        &payload.initial_dysfunction,
        payload.severity,
    );
    axum::Json(cascade)
}

// Chronos handlers: CGM ingestion + variability

#[derive(serde::Deserialize)]
struct CgmSourceQuery {
    source: String,
}

#[axum::debug_handler]
async fn ingest_cgm(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    Query(params): Query<CgmSourceQuery>,
    mut multipart: Multipart,
) -> Result<axum::Json<serde_json::Value>> {
    let mut content = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
    {
        if field.name().unwrap_or("") == "file" {
            content = Some(
                field
                    .text()
                    .await
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?,
            );
        }
    }
    let content = content.ok_or_else(|| ApiError::BadRequest("missing field: file".to_string()))?;

    let parse_result = match params.source.as_str() {
        "dexcom" => chronos_cgm::dexcom::parse_dexcom_csv(&content),
        "libre" => chronos_cgm::libre::parse_libre_csv(&content),
        other => {
            return Err(ApiError::BadRequest(format!(
                "unknown CGM source: {}",
                other
            )));
        }
    };

    let mut readings = match parse_result {
        Ok(readings) => readings,
        Err(e) => {
            let quarantine_key = format!("quarantine/cgm/{}.csv", uuid::Uuid::new_v4());
            let _ = state
                .object_store
                .upload_to_quarantine(&quarantine_key, content.into_bytes())
                .await;
            return Err(ApiError::BadRequest(format!(
                "{} (quarantined as {})",
                e, quarantine_key
            )));
        }
    };

    chronos_cgm::validate_cgm_readings(&readings)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    chronos_cgm::sort_readings(&mut readings);

    let count = chronos_storage::insert_cgm_readings(&state.pool, &readings).await?;

    Ok(axum::Json(serde_json::json!({
        "status": "ok",
        "readings_ingested": count,
    })))
}

#[axum::debug_handler]
async fn get_cgm_readings(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(subject_id): axum::extract::Path<String>,
) -> Result<axum::Json<Vec<tpt_soma_core::query::CgmReadingRecord>>> {
    let records =
        tpt_soma_core::query::get_cgm_readings_by_subject(&state.pool, &subject_id).await?;
    Ok(axum::Json(records))
}

#[derive(serde::Deserialize)]
struct VariabilityQuery {
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
}

#[axum::debug_handler]
async fn get_cgm_variability(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(subject_id): axum::extract::Path<String>,
    Query(params): Query<VariabilityQuery>,
) -> Result<axum::Json<chronos_variability::GlycemicVariability>> {
    let records = tpt_soma_core::query::get_cgm_readings_in_range(
        &state.pool,
        &subject_id,
        params.start,
        params.end,
    )
    .await?;

    if records.is_empty() {
        return Err(ApiError::NotFound(format!(
            "no CGM readings for subject {} in range",
            subject_id
        )));
    }

    let values: Vec<f64> = records.iter().map(|r| r.glucose_mgdl).collect();
    let timestamps: Vec<chrono::DateTime<chrono::Utc>> = records.iter().map(|r| r.ts).collect();

    let variability = chronos_variability::calculate_glycemic_variability(
        &subject_id,
        &values,
        &timestamps,
        params.start,
        params.end,
    )
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(axum::Json(variability))
}

// Phase 4: systemic query builder over the OSG cross-talk solver

#[derive(serde::Deserialize)]
struct SystemicNodeDto {
    id: String,
    kind: String,
}

#[derive(serde::Deserialize)]
struct SystemicEdgeDto {
    from: String,
    to: String,
    edge_type: String,
}

#[derive(serde::Deserialize)]
struct SystemicQueryRequest {
    nodes: Vec<SystemicNodeDto>,
    edges: Vec<SystemicEdgeDto>,
    target_node_id: String,
    t0: f64,
    dt: f64,
    steps: usize,
    adipose_secretion: f64,
    igf1_clearance: f64,
    igf1_growth: f64,
}

#[axum::debug_handler]
async fn run_systemic_query(
    axum::extract::Json(payload): axum::extract::Json<SystemicQueryRequest>,
) -> Result<axum::Json<serde_json::Value>> {
    let query = crosstalk::SystemicQuery {
        topology: crosstalk::OsTopology {
            nodes: payload
                .nodes
                .into_iter()
                .map(|n| crosstalk::OsNodeRow {
                    id: n.id,
                    kind: n.kind,
                })
                .collect(),
            edges: payload
                .edges
                .into_iter()
                .map(|e| crosstalk::OsEdgeRow {
                    from: e.from,
                    to: e.to,
                    edge_type: e.edge_type,
                })
                .collect(),
        },
        params: crosstalk::CrossTalkParams {
            adipose_secretion: payload.adipose_secretion,
            igf1_clearance: payload.igf1_clearance,
            igf1_growth: payload.igf1_growth,
        },
        target_node_id: payload.target_node_id,
        t0: payload.t0,
        dt: payload.dt,
        steps: payload.steps,
    };
    // Every simulacrum input error is a caller error (unknown kind, dangling edge, bad window).
    let result =
        crosstalk::run_systemic_query(&query).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(axum::Json(serde_json::json!({
        "target_node_id": result.target_node_id,
        "final_value": result.final_value,
        "peak_value": result.peak_value,
        "peak_time": result.peak_time,
        "samples": result.samples,
    })))
}

// Phase 4: clinical trial handlers

fn clinica_err(e: ClinicaError) -> ApiError {
    match e {
        ClinicaError::InvalidInput(m) => ApiError::BadRequest(m),
        ClinicaError::Database(e) => ApiError::Database(e),
        other => ApiError::Internal(other.to_string()),
    }
}

fn parse_trial_id(raw: &str) -> Result<Uuid> {
    Uuid::parse_str(raw).map_err(|e| ApiError::BadRequest(format!("invalid trial id: {e}")))
}

#[derive(serde::Deserialize)]
struct CriterionDto {
    /// `inclusion` or `exclusion`
    kind: String,
    key: String,
    min: f64,
    max: f64,
}

impl CriterionDto {
    fn into_criterion(self) -> Result<Criterion> {
        let Self {
            kind,
            key,
            min,
            max,
        } = self;
        match kind.as_str() {
            "inclusion" => Ok(Criterion::Inclusion { key, min, max }),
            "exclusion" => Ok(Criterion::Exclusion { key, min, max }),
            other => Err(ApiError::BadRequest(format!(
                "criterion kind must be inclusion or exclusion, got {other}"
            ))),
        }
    }
}

#[derive(serde::Deserialize)]
struct ArmDto {
    name: String,
    #[serde(default = "default_allocation_ratio")]
    allocation_ratio: u32,
}

fn default_allocation_ratio() -> u32 {
    1
}

#[derive(serde::Deserialize)]
struct VisitDto {
    name: String,
    /// Days after enrollment. Day 0 is the enrollment day.
    day: u32,
}

#[derive(serde::Deserialize)]
struct CreateTrialRequest {
    trial_name: String,
    cohort_label: String,
    criteria: Vec<CriterionDto>,
    #[serde(default)]
    arms: Vec<ArmDto>,
    #[serde(default)]
    visits: Vec<VisitDto>,
}

#[axum::debug_handler]
async fn create_trial(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Json(payload): axum::extract::Json<CreateTrialRequest>,
) -> Result<axum::Json<serde_json::Value>> {
    let criteria = payload
        .criteria
        .into_iter()
        .map(CriterionDto::into_criterion)
        .collect::<Result<Vec<_>>>()?;
    let design = ProtocolDesign {
        arms: payload
            .arms
            .into_iter()
            .map(|a| Arm {
                name: a.name,
                allocation_ratio: a.allocation_ratio,
            })
            .collect(),
        visits: payload
            .visits
            .into_iter()
            .map(|v| Visit {
                name: v.name,
                day: v.day,
            })
            .collect(),
    };
    let id = clinica_storage::save_trial_protocol_design(
        &state.pool,
        &payload.trial_name,
        &payload.cohort_label,
        &criteria,
        &design,
    )
    .await
    .map_err(clinica_err)?;
    Ok(axum::Json(serde_json::json!({ "trial_id": id })))
}

/// Visit dates for one enrolled subject, counted from their enrollment date.
#[axum::debug_handler]
async fn get_subject_schedule(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path((trial_id, subject_id)): axum::extract::Path<(String, String)>,
) -> Result<axum::Json<serde_json::Value>> {
    let trial_id = parse_trial_id(&trial_id)?;
    let enrollment = clinica_storage::load_subject_enrollment(&state.pool, trial_id, &subject_id)
        .await
        .map_err(clinica_err)?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "subject {subject_id} is not enrolled in trial {trial_id}"
            ))
        })?;
    let design = clinica_storage::load_trial_design(&state.pool, trial_id)
        .await
        .map_err(clinica_err)?;
    let visits = visit_schedule(&design, enrollment.enrolled_on).map_err(clinica_err)?;
    Ok(axum::Json(serde_json::json!({
        "subject_id": subject_id,
        "arm": enrollment.arm,
        "enrolled_on": enrollment.enrolled_on,
        "visits": visits
            .iter()
            .map(|(name, date)| serde_json::json!({ "visit": name, "date": date }))
            .collect::<Vec<_>>(),
    })))
}

#[derive(serde::Deserialize)]
struct ScreenCandidateDto {
    subject_id: String,
    features: SubjectFeatures,
}

#[derive(serde::Deserialize)]
struct ScreenTrialRequest {
    candidates: Vec<ScreenCandidateDto>,
    target: usize,
}

#[axum::debug_handler]
async fn screen_trial(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(trial_id): axum::extract::Path<String>,
    axum::extract::Json(payload): axum::extract::Json<ScreenTrialRequest>,
) -> Result<axum::Json<serde_json::Value>> {
    let trial_id = parse_trial_id(&trial_id)?;
    let candidates: Vec<(String, SubjectFeatures)> = payload
        .candidates
        .into_iter()
        .map(|c| (c.subject_id, c.features))
        .collect();
    let screened = candidates.len();
    let enrolled =
        clinica_storage::screen_and_persist(&state.pool, trial_id, &candidates, payload.target)
            .await
            .map_err(clinica_err)?;
    Ok(axum::Json(serde_json::json!({
        "screened": screened,
        "enrolled": enrolled,
        "shortfall": payload.target.saturating_sub(enrolled),
    })))
}

#[derive(serde::Deserialize)]
struct AdverseEventDto {
    subject_id: String,
    term: String,
    grade: u8,
    serious: bool,
}

#[derive(serde::Deserialize)]
struct RecordAdverseEventsRequest {
    events: Vec<AdverseEventDto>,
}

#[axum::debug_handler]
async fn record_trial_adverse_events(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(trial_id): axum::extract::Path<String>,
    axum::extract::Json(payload): axum::extract::Json<RecordAdverseEventsRequest>,
) -> Result<axum::Json<serde_json::Value>> {
    let trial_id = parse_trial_id(&trial_id)?;
    let events: Vec<AdverseEvent> = payload
        .events
        .into_iter()
        .map(|e| AdverseEvent {
            subject_id: e.subject_id,
            term: e.term,
            grade: e.grade,
            serious: e.serious,
        })
        .collect();
    let recorded = clinica_storage::record_adverse_events(&state.pool, trial_id, &events)
        .await
        .map_err(clinica_err)?;
    Ok(axum::Json(serde_json::json!({ "recorded": recorded })))
}

#[axum::debug_handler]
async fn get_trial_ae_summary(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(trial_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>> {
    let trial_id = parse_trial_id(&trial_id)?;
    let s = clinica_storage::load_ae_summary(&state.pool, trial_id)
        .await
        .map_err(clinica_err)?;
    Ok(axum::Json(serde_json::json!({
        "total": s.total,
        "by_grade": s.by_grade,
        "grade_3_or_higher": s.grade_3_or_higher,
        "serious": s.serious,
        "subjects_with_ae": s.subjects_with_ae,
    })))
}

// Cross-phase handler: Phase 1 samples + Phase 2 clinical/CGM records for one subject

#[axum::debug_handler]
async fn get_cross_phase_summary(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(patient_id): axum::extract::Path<String>,
) -> Result<axum::Json<tpt_soma_core::query::CrossPhaseSubjectSummary>> {
    let summary =
        tpt_soma_core::query::get_cross_phase_subject_summary(&state.pool, &patient_id).await?;
    Ok(axum::Json(summary))
}

// Phase 3: simulation handlers

#[derive(serde::Deserialize)]
struct SimulateRequest {
    subject_id: String,
    t0: f64,
    dt: f64,
    steps: usize,
    adipose_secretion: f64,
    igf1_clearance: f64,
    igf1_growth: f64,
}

#[axum::debug_handler]
async fn run_simulation(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Json(payload): axum::extract::Json<SimulateRequest>,
) -> Result<axum::Json<serde_json::Value>> {
    if payload.dt <= 0.0 || payload.steps == 0 {
        return Err(ApiError::BadRequest(
            "dt must be positive and steps must be > 0".to_string(),
        ));
    }
    let (system, init) = crosstalk::build_adipose_igf1_breast(
        payload.adipose_secretion,
        payload.igf1_clearance,
        payload.igf1_growth,
    );
    let traj = system.simulate(payload.t0, &init, payload.dt, payload.steps);
    let run_id = Uuid::new_v4();
    let base = chrono::Utc::now();
    let run = sim_storage::SimulationRun {
        id: run_id,
        subject_id: payload.subject_id.clone(),
        model_name: "adipose_igf1_breast".to_string(),
        created_at: base,
        status: "completed".to_string(),
    };
    sim_storage::insert_simulation_run(&state.pool, &run)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let series = ["adipose", "igf1", "breast"];
    for (t, y) in &traj {
        let ts = base + chrono::Duration::milliseconds((t * 1000.0) as i64);
        for (j, name) in series.iter().enumerate() {
            let out = sim_storage::SimulationOutput {
                id: Uuid::new_v4(),
                run_id,
                ts,
                series_name: name.to_string(),
                value: y[j],
            };
            sim_storage::insert_simulation_output(&state.pool, &out)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
        }
    }

    Ok(axum::Json(serde_json::json!({
        "run_id": run_id,
        "model": run.model_name,
        "steps": traj.len(),
    })))
}

#[axum::debug_handler]
async fn get_simulation(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Path(run_id): axum::extract::Path<String>,
) -> Result<axum::Json<Vec<sim_storage::SimulationOutput>>> {
    let run_uuid = Uuid::parse_str(&run_id).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let outs = sim_storage::get_simulation_outputs(&state.pool, run_uuid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(axum::Json(outs))
}

#[derive(serde::Deserialize)]
struct SimulationAggregateRequest {
    run_id: String,
    sensitivity: Option<f64>,
}

#[axum::debug_handler]
async fn simulation_aggregate_count(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Json(payload): axum::extract::Json<SimulationAggregateRequest>,
) -> Result<axum::Json<serde_json::Value>> {
    let run_uuid =
        Uuid::parse_str(&payload.run_id).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM simulation_outputs WHERE run_id = $1")
            .bind(run_uuid)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    let sensitivity = payload.sensitivity.unwrap_or(1.0);
    let noisy = release_dp_count(
        &state,
        &payload.run_id,
        "simulation",
        count as usize,
        sensitivity,
    )
    .await?;
    Ok(axum::Json(serde_json::json!({
        "run_id": payload.run_id,
        "noisy_count": noisy,
        "epsilon_spent": sensitivity,
    })))
}

#[derive(serde::Deserialize)]
struct CrossDomainAggregateRequest {
    cohort_id: String,
    sensitivity: Option<f64>,
}

/// Cross-domain aggregate export (ADR 007 §2.8): release a single
/// differentially-private count that combines multiple Phase 1–4 data classes
/// (samples, variant records, expression records) for one cohort. The full
/// cohort epsilon budget is spent once for the combined export.
#[axum::debug_handler]
async fn cross_domain_aggregate_count(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Json(payload): axum::extract::Json<CrossDomainAggregateRequest>,
) -> Result<axum::Json<serde_json::Value>> {
    let sample_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cohort_membership WHERE cohort_id = $1")
            .bind(&payload.cohort_id)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

    let variant_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sample_variants sv \
         JOIN cohort_membership cm ON sv.sample_id = cm.sample_id \
         WHERE cm.cohort_id = $1",
    )
    .bind(&payload.cohort_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let expression_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM scrna_expression e \
         JOIN cohort_membership cm ON e.sample_id = cm.sample_id \
         WHERE cm.cohort_id = $1",
    )
    .bind(&payload.cohort_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let domains = [
        ("cohort_membership".to_string(), sample_count as usize),
        ("genomic_variant".to_string(), variant_count as usize),
        (
            "transcriptomic_scrna".to_string(),
            expression_count as usize,
        ),
    ];

    let sensitivity = payload.sensitivity.unwrap_or(1.0);
    let total: usize = domains.iter().map(|(_, count)| *count).sum();
    let noisy = release_dp_count(&state, &payload.cohort_id, "api", total, sensitivity).await?;

    Ok(axum::Json(serde_json::json!({
        "cohort_id": payload.cohort_id,
        "domain_counts": {
            "cohort_membership": sample_count,
            "genomic_variant": variant_count,
            "transcriptomic_scrna": expression_count,
        },
        "noisy_total": noisy,
        "epsilon_spent": sensitivity,
    })))
}

async fn metrics_handler() -> impl IntoResponse {
    axum::http::Response::builder()
        .header("Content-Type", crate::metrics::CONTENT_TYPE)
        .body(axum::body::Body::from(crate::metrics::render()))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::required_capability_for;
    use axum::body::Body;
    use axum::extract::Multipart;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use tower::ServiceExt;

    /// `(method, path)` for each `.route(...)` call in a source file. Only the
    /// text before the test module is scanned, so this test cannot read itself.
    fn registered_routes(source: &str, prefix: &str) -> Vec<(&'static str, String)> {
        let source = source.split("#[cfg(test)]").next().unwrap_or(source);
        let mut routes = Vec::new();
        for part in source.split(".route(").skip(1) {
            let head = part.get(..200).unwrap_or(part);
            let Some(path) = head.split('"').nth(1) else {
                continue;
            };
            let method = match (head.find("get("), head.find("post(")) {
                (Some(g), Some(p)) if g < p => "GET",
                (Some(_), Some(_)) | (None, Some(_)) => "POST",
                (Some(_), None) => "GET",
                (None, None) => continue,
            };
            routes.push((method, format!("{prefix}{path}")));
        }
        routes
    }

    /// Replaces `:param` segments with a concrete value the policy patterns match.
    fn concrete(path: &str) -> String {
        path.split('/')
            .map(|seg| if seg.starts_with(':') { "x" } else { seg })
            .collect::<Vec<_>>()
            .join("/")
    }

    #[test]
    fn every_registered_route_has_a_policy_entry() {
        let mut routes = registered_routes(include_str!("server.rs"), "");
        routes.extend(registered_routes(
            include_str!("../../tpt-soma-ingest/src/endpoint.rs"),
            "/api/v1",
        ));
        routes.retain(|(_, path)| path.starts_with("/api/v1"));
        assert!(routes.len() >= 30, "route scan found only {}", routes.len());

        let uncovered: Vec<_> = routes
            .iter()
            .filter(|(method, path)| required_capability_for(method, &concrete(path)).is_none())
            .collect();
        assert!(
            uncovered.is_empty(),
            "routes without a POLICY entry: {uncovered:?}"
        );
    }

    /// Converts axum's `:param` segments to OpenAPI's `{param}`.
    fn openapi_path(path: &str) -> String {
        path.split('/')
            .map(|seg| match seg.strip_prefix(':') {
                Some(name) => format!("{{{name}}}"),
                None => seg.to_string(),
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    /// docs/api/openapi.json must list exactly the (method, path) pairs the
    /// router registers, so the documented API cannot drift from the real one.
    #[test]
    fn openapi_spec_matches_registered_routes() {
        use std::collections::BTreeSet;

        let mut routes = registered_routes(include_str!("server.rs"), "");
        routes.extend(registered_routes(
            include_str!("../../tpt-soma-ingest/src/endpoint.rs"),
            "/api/v1",
        ));
        let registered: BTreeSet<(String, String)> = routes
            .into_iter()
            .map(|(method, path)| (method.to_string(), openapi_path(&path)))
            .collect();

        let spec: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/api/openapi.json"))
                .expect("docs/api/openapi.json must be valid JSON");
        let documented: BTreeSet<(String, String)> = spec["paths"]
            .as_object()
            .expect("spec has a paths object")
            .iter()
            .flat_map(|(path, item)| {
                item.as_object()
                    .into_iter()
                    .flatten()
                    .map(move |(method, _)| (method.to_uppercase(), path.clone()))
            })
            .collect();

        let undocumented: Vec<_> = registered.difference(&documented).collect();
        let stale: Vec<_> = documented.difference(&registered).collect();
        assert!(
            undocumented.is_empty() && stale.is_empty(),
            "undocumented routes: {undocumented:?}; spec entries with no route: {stale:?}"
        );
    }

    fn upload_request(size: usize) -> Request<Body> {
        let boundary = "tptsomaboundary";
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"big.vcf\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .into_bytes();
        body.extend(std::iter::repeat_n(b'x', size));
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        Request::builder()
            .method("POST")
            .uri("/upload")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap()
    }

    /// Reads every part in full. Limit errors surface here as 413, because the
    /// size check runs while the body is read.
    async fn upload(
        mut multipart: Multipart,
    ) -> std::result::Result<&'static str, (StatusCode, String)> {
        let too_large = |e: axum::extract::multipart::MultipartError| {
            (StatusCode::PAYLOAD_TOO_LARGE, e.to_string())
        };
        while let Some(field) = multipart.next_field().await.map_err(too_large)? {
            field.bytes().await.map_err(too_large)?;
        }
        Ok("ok")
    }

    /// Control: axum's own 2 MB default rejects a 3 MB upload. This is the limit
    /// that rejected real VCF and h5ad files before the cap was raised.
    #[tokio::test]
    async fn default_multipart_limit_is_2_mb() {
        let app = Router::new().route("/upload", post(upload));
        let response = app.oneshot(upload_request(3 * 1024 * 1024)).await.unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn body_limit_rejects_uploads_over_the_cap() {
        let app = apply_body_limit(Router::new().route("/upload", post(upload)), 1024 * 1024);
        let response = app.oneshot(upload_request(3 * 1024 * 1024)).await.unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn body_limit_admits_uploads_under_the_cap() {
        let app = apply_body_limit(Router::new().route("/upload", post(upload)), 1024 * 1024);
        let response = app.oneshot(upload_request(512 * 1024)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}

/// Trial handlers against a real database. The capability layer is left out on
/// purpose: its policy is covered in `auth.rs`, and these tests target the
/// handlers' own wiring of request, storage and response.
#[cfg(test)]
mod trial_handler_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use tower::ServiceExt;

    async fn trial_app() -> std::result::Result<Router, Box<dyn std::error::Error + Send + Sync>> {
        let pool = tpt_soma_core::test_helpers::test_pool().await?;
        let state = Arc::new(AuthState {
            pool: pool.clone(),
            verifying_key: ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]).verifying_key(),
            revocation_list: Arc::new(RevocationList::new()),
            audit_ledger: Arc::new(AuditLedger::new(pool)),
            dp_service: Arc::new(tokio::sync::Mutex::new(
                tpt_soma_core::DifferentialPrivacyService::new(1.0),
            )),
            object_store: Arc::new(tpt_soma_core::store::ObjectStoreClient::from_env()),
        });
        Ok(Router::new()
            .route("/api/v1/trials", post(create_trial))
            .route("/api/v1/trials/{trial_id}/screen", post(screen_trial))
            .route(
                "/api/v1/trials/{trial_id}/adverse-events",
                post(record_trial_adverse_events),
            )
            .route(
                "/api/v1/trials/{trial_id}/adverse-events/summary",
                get(get_trial_ae_summary),
            )
            .route(
                "/api/v1/trials/{trial_id}/subjects/{subject_id}/schedule",
                get(get_subject_schedule),
            )
            .with_state(state))
    }

    async fn send(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_trial_handlers_create_screen_schedule_and_report_ae()
    -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let app = trial_app().await?;
        // Unique names per run, so the test never depends on rows left by an earlier run.
        let run = Uuid::new_v4().simple().to_string();

        let (status, created) = send(
            &app,
            "POST",
            "/api/v1/trials",
            Some(serde_json::json!({
                "trial_name": format!("handler-{run}"),
                "cohort_label": format!("cohort-{run}"),
                "criteria": [{ "kind": "inclusion", "key": "hba1c", "min": 6.5, "max": 10.0 }],
                "arms": [
                    { "name": "treatment", "allocation_ratio": 1 },
                    { "name": "control", "allocation_ratio": 1 }
                ],
                "visits": [
                    { "name": "baseline", "day": 0 },
                    { "name": "week4", "day": 28 }
                ]
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let trial_id = created["trial_id"].as_str().unwrap().to_string();

        // Two eligible subjects fill the target of 2. The third fails the inclusion criterion.
        let eligible = |id: &str| {
            serde_json::json!({
                "subject_id": id,
                "features": { "hba1c": 7.0 }
            })
        };
        let (status, screened) = send(
            &app,
            "POST",
            &format!("/api/v1/trials/{trial_id}/screen"),
            Some(serde_json::json!({
                "candidates": [
                    eligible(&format!("h1-{run}")),
                    eligible(&format!("h2-{run}")),
                    { "subject_id": format!("h3-{run}"), "features": { "hba1c": 4.0 } }
                ],
                "target": 2
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(screened["screened"], 3);
        assert_eq!(screened["enrolled"], 2);
        assert_eq!(screened["shortfall"], 0);

        // Arms follow the 1:1 cycle in enrollment order.
        let (status, schedule) = send(
            &app,
            "GET",
            &format!("/api/v1/trials/{trial_id}/subjects/h1-{run}/schedule"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(schedule["arm"], "treatment");
        let visits = schedule["visits"].as_array().unwrap();
        assert_eq!(visits.len(), 2);
        assert_eq!(visits[1]["visit"], "week4");

        let (status, _) = send(
            &app,
            "GET",
            &format!("/api/v1/trials/{trial_id}/subjects/h2-{run}/schedule"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // The screened-out subject has no enrollment, so there is no schedule.
        let (status, _) = send(
            &app,
            "GET",
            &format!("/api/v1/trials/{trial_id}/subjects/h3-{run}/schedule"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, recorded) = send(
            &app,
            "POST",
            &format!("/api/v1/trials/{trial_id}/adverse-events"),
            Some(serde_json::json!({
                "events": [{
                    "subject_id": format!("h1-{run}"),
                    "term": "nausea",
                    "grade": 3,
                    "serious": true
                }]
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(recorded["recorded"], 1);

        let (status, summary) = send(
            &app,
            "GET",
            &format!("/api/v1/trials/{trial_id}/adverse-events/summary"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(summary["total"], 1);
        assert_eq!(summary["grade_3_or_higher"], 1);
        assert_eq!(summary["serious"], 1);
        assert_eq!(summary["subjects_with_ae"], 1);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_trial_handler_rejects_malformed_trial_id()
    -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let app = trial_app().await?;
        let (status, _) = send(
            &app,
            "GET",
            "/api/v1/trials/not-a-uuid/adverse-events/summary",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        Ok(())
    }
}
