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

use axum::extract::{Multipart, Query};
use tpt_soma_chronos::{
    cgm as chronos_cgm, storage as chronos_storage, trajectory as chronos_trajectory,
    variability as chronos_variability,
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
            object_store: self.object_store.clone(),
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
            // Organon: clinical observation ingestion + trajectories
            .route(
                "/api/v1/ingest/fhir-observation",
                post(ingest_fhir_observation),
            )
            .route("/api/v1/ingest/organ-csv", post(ingest_organ_csv))
            .route(
                "/api/v1/clinical-observations/:subject_id",
                get(get_clinical_observations),
            )
            .route(
                "/api/v1/clinical-observations/:subject_id/:loinc_code/trajectory",
                get(get_organ_trajectory),
            )
            // Organon: imaging ingestion + listing
            .route("/api/v1/ingest/imaging", post(ingest_organ_imaging))
            .route("/api/v1/organ-imaging/:subject_id", get(get_organ_imaging))
            // Organon: cross-organ coupling graph
            .route("/api/v1/organ-system-graph", get(get_organ_system_graph))
            .route(
                "/api/v1/organ-system-graph/cascade",
                post(simulate_organ_cascade),
            )
            // Chronos: CGM ingestion + variability
            .route("/api/v1/ingest/cgm", post(ingest_cgm))
            .route("/api/v1/cgm/:subject_id", get(get_cgm_readings))
            .route(
                "/api/v1/cgm/:subject_id/variability",
                get(get_cgm_variability),
            )
            // Cross-phase: Phase 1 samples + Phase 2 clinical/CGM records for one subject
            .route(
                "/api/v1/subjects/:patient_id/cross-phase-summary",
                get(get_cross_phase_summary),
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

// Organon handlers: clinical observation ingestion + trajectories

#[axum::debug_handler]
async fn ingest_fhir_observation(
    axum::extract::State(state): axum::extract::State<Arc<AuthState>>,
    axum::extract::Json(payload): axum::extract::Json<organon_ingestion::FhirObservation>,
) -> Result<axum::Json<serde_json::Value>> {
    let observation = organon_ingestion::parse_fhir_observation(&payload)?;
    let raw_payload = serde_json::to_value(&payload).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    organon_storage::insert_clinical_observation(&state.pool, &observation, "fhir").await?;
    organon_storage::insert_fhir_resource_payload(&state.pool, "Observation", &payload.id, &raw_payload)
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
    let count = organon_storage::insert_clinical_observations(&state.pool, &observations, "csv").await?;

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
        tpt_soma_core::query::get_clinical_observations_by_subject(&state.pool, &subject_id).await?;
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

    let metadata = metadata.ok_or_else(|| ApiError::BadRequest("missing field: metadata".to_string()))?;
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

    let record = organon_imaging::OrganImagingRecord {
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
            modality: metadata.modality.parse().unwrap_or(organon_imaging::ImagingModality::Other(
                "unknown".to_string(),
            )),
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
    let records = tpt_soma_core::query::get_organ_imaging_by_subject(&state.pool, &subject_id).await?;
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
        other => return Err(ApiError::BadRequest(format!("unknown CGM source: {}", other))),
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

    chronos_cgm::validate_cgm_readings(&readings).map_err(|e| ApiError::BadRequest(e.to_string()))?;
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
    let records = tpt_soma_core::query::get_cgm_readings_by_subject(&state.pool, &subject_id).await?;
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
