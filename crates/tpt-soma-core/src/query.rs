use crate::connection::Result;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

pub async fn graph_neighbors(
    pool: &PgPool,
    node_id: &str,
    edge_label: &str,
) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>("SELECT target_id FROM graph_neighbors($1, $2)")
        .bind(node_id)
        .bind(edge_label)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn graph_bfs(
    pool: &PgPool,
    start_id: &str,
    max_depth: i32,
) -> Result<Vec<(String, i32)>> {
    let rows = sqlx::query_as("SELECT node_id, depth FROM graph_bfs($1, $2)")
        .bind(start_id)
        .bind(max_depth)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn plex_match(pool: &PgPool, pattern: &str) -> Result<Vec<Value>> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT row_to_json(t)::text::jsonb FROM plex_match($1) AS t",
    )
    .bind(pattern)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// Phase 1 query helpers

pub async fn get_variants_by_sample(pool: &PgPool, sample_id: &str) -> Result<Vec<VariantRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT v.variant_id, v.chromosome, v.position, v.reference, v.alternate, v.rsid, v.clinvar_id, sv.genotype
        FROM variants v
        JOIN sample_variants sv ON v.variant_id = sv.variant_id
        WHERE sv.sample_id = $1
        "#,
    )
    .bind(sample_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_expression_by_sample(
    pool: &PgPool,
    sample_id: &str,
) -> Result<Vec<ExpressionRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT sample_id, cell_id, gene_id, count
        FROM scrna_expression
        WHERE sample_id = $1
        "#,
    )
    .bind(sample_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_expression_by_gene(pool: &PgPool, gene_id: &str) -> Result<Vec<ExpressionRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT sample_id, cell_id, gene_id, count
        FROM scrna_expression
        WHERE gene_id = $1
        "#,
    )
    .bind(gene_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn join_variant_expression(
    pool: &PgPool,
    sample_id: &str,
    variant_id: &str,
    gene_id: &str,
) -> Result<Vec<VariantExpressionJoin>> {
    let rows = sqlx::query_as(
        r#"
        SELECT v.variant_id, v.chromosome, v.position, v.reference, v.alternate, v.rsid,
               e.cell_id, e.gene_id, e.count
        FROM variants v
        JOIN sample_variants sv ON v.variant_id = sv.variant_id
        JOIN scrna_expression e ON sv.sample_id = e.sample_id
        WHERE sv.sample_id = $1
          AND v.variant_id = $2
          AND e.gene_id = $3
        "#,
    )
    .bind(sample_id)
    .bind(variant_id)
    .bind(gene_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_cohort_samples(pool: &PgPool, cohort_id: &str) -> Result<Vec<SampleRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT s.sample_id, s.patient_id, s.source, s.dataset_provenance
        FROM samples s
        JOIN cohort_membership cm ON s.sample_id = cm.sample_id
        WHERE cm.cohort_id = $1
        "#,
    )
    .bind(cohort_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_umap_by_sample(pool: &PgPool, sample_id: &str) -> Result<Vec<UmapRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT sample_id, cell_id, umap1, umap2, cluster
        FROM scrna_umap
        WHERE sample_id = $1
        "#,
    )
    .bind(sample_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_umap_by_cluster(
    pool: &PgPool,
    sample_id: &str,
    cluster: &str,
) -> Result<Vec<UmapRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT sample_id, cell_id, umap1, umap2, cluster
        FROM scrna_umap
        WHERE sample_id = $1 AND cluster = $2
        "#,
    )
    .bind(sample_id)
    .bind(cluster)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct VariantRecord {
    pub variant_id: String,
    pub chromosome: String,
    pub position: i32,
    pub reference: String,
    pub alternate: String,
    pub rsid: Option<String>,
    pub clinvar_id: Option<String>,
    pub genotype: Option<String>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ExpressionRecord {
    pub sample_id: uuid::Uuid,
    pub cell_id: String,
    pub gene_id: String,
    pub count: i32,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct VariantExpressionJoin {
    pub variant_id: String,
    pub chromosome: String,
    pub position: i32,
    pub reference: String,
    pub alternate: String,
    pub rsid: Option<String>,
    pub cell_id: String,
    pub gene_id: String,
    pub count: i32,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct SampleRecord {
    pub sample_id: uuid::Uuid,
    pub patient_id: Option<uuid::Uuid>,
    pub source: String,
    pub dataset_provenance: Option<String>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct UmapRecord {
    pub sample_id: uuid::Uuid,
    pub cell_id: String,
    pub umap1: f64,
    pub umap2: f64,
    pub cluster: String,
}

// Phase 2 query helpers (organon/chronos)

pub async fn get_clinical_observations_by_subject(
    pool: &PgPool,
    subject_id: &str,
) -> Result<Vec<ClinicalObservationRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT id, subject_id, loinc_code, value, unit, effective_time, status, interpretation, source
        FROM organ_function_observations
        WHERE subject_id = $1
        ORDER BY effective_time
        "#,
    )
    .bind(subject_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_clinical_observations_by_subject_and_loinc(
    pool: &PgPool,
    subject_id: &str,
    loinc_code: &str,
) -> Result<Vec<ClinicalObservationRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT id, subject_id, loinc_code, value, unit, effective_time, status, interpretation, source
        FROM organ_function_observations
        WHERE subject_id = $1 AND loinc_code = $2
        ORDER BY effective_time
        "#,
    )
    .bind(subject_id)
    .bind(loinc_code)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_cgm_readings_by_subject(
    pool: &PgPool,
    subject_id: &str,
) -> Result<Vec<CgmReadingRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT id, subject_id, ts, glucose_mgdl, source, sensor_id, is_calibrated, trend_arrow
        FROM cgm_readings
        WHERE subject_id = $1
        ORDER BY ts
        "#,
    )
    .bind(subject_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_cgm_readings_in_range(
    pool: &PgPool,
    subject_id: &str,
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<CgmReadingRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT id, subject_id, ts, glucose_mgdl, source, sensor_id, is_calibrated, trend_arrow
        FROM cgm_readings
        WHERE subject_id = $1 AND ts >= $2 AND ts <= $3
        ORDER BY ts
        "#,
    )
    .bind(subject_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_organ_imaging_by_subject(
    pool: &PgPool,
    subject_id: &str,
) -> Result<Vec<OrganImagingRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT id, subject_id, study_instance_uid, series_instance_uid, sop_instance_uid,
               modality, body_part_examined, organ_system, laterality,
               minio_bucket, minio_object_key, checksum_sha256, file_size_bytes, ingested_at
        FROM organ_imaging_records
        WHERE subject_id = $1
        ORDER BY ingested_at
        "#,
    )
    .bind(subject_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ClinicalObservationRecord {
    pub id: uuid::Uuid,
    pub subject_id: String,
    pub loinc_code: String,
    pub value: f64,
    pub unit: Option<String>,
    pub effective_time: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub interpretation: Vec<String>,
    pub source: String,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct CgmReadingRecord {
    pub id: uuid::Uuid,
    pub subject_id: String,
    pub ts: chrono::DateTime<chrono::Utc>,
    pub glucose_mgdl: f64,
    pub source: String,
    pub sensor_id: Option<String>,
    pub is_calibrated: bool,
    pub trend_arrow: Option<String>,
}

/// Cross-phase integration: a subject linked across Phase 1 genomic/cytos samples
/// (by patient_id) and Phase 2 clinical/CGM records (by subject_id), in one query path.
pub async fn get_cross_phase_subject_summary(
    pool: &PgPool,
    patient_id: &str,
) -> Result<CrossPhaseSubjectSummary> {
    let sample_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM samples WHERE patient_id = $1::uuid")
            .bind(patient_id)
            .fetch_one(pool)
            .await?;

    let observation_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM organ_function_observations WHERE subject_id = $1",
    )
    .bind(patient_id)
    .fetch_one(pool)
    .await?;

    let cgm_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cgm_readings WHERE subject_id = $1")
            .bind(patient_id)
            .fetch_one(pool)
            .await?;

    Ok(CrossPhaseSubjectSummary {
        patient_id: patient_id.to_string(),
        phase1_sample_count: sample_count,
        phase2_observation_count: observation_count,
        phase2_cgm_reading_count: cgm_count,
    })
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct CrossPhaseSubjectSummary {
    pub patient_id: String,
    pub phase1_sample_count: i64,
    pub phase2_observation_count: i64,
    pub phase2_cgm_reading_count: i64,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct OrganImagingRecord {
    pub id: uuid::Uuid,
    pub subject_id: String,
    pub study_instance_uid: String,
    pub series_instance_uid: String,
    pub sop_instance_uid: String,
    pub modality: String,
    pub body_part_examined: Option<String>,
    pub organ_system: Option<String>,
    pub laterality: Option<String>,
    pub minio_bucket: String,
    pub minio_object_key: String,
    pub checksum_sha256: String,
    pub file_size_bytes: i64,
    pub ingested_at: chrono::DateTime<chrono::Utc>,
}
