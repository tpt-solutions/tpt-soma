//! Persistence for clinical observations, raw FHIR payloads, and organ imaging metadata

use crate::imaging::OrganImagingRecord;
use crate::ingestion::ClinicalObservation;
use crate::{OrganonError, Result};
use sqlx::PgPool;

/// Insert a normalized clinical observation (from FHIR or CSV ingestion)
pub async fn insert_clinical_observation(
    pool: &PgPool,
    obs: &ClinicalObservation,
    source: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO organ_function_observations
            (id, subject_id, loinc_code, value, unit, effective_time, status, interpretation, source)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(obs.id)
    .bind(&obs.subject_id)
    .bind(&obs.loinc_code)
    .bind(obs.value)
    .bind(&obs.unit)
    .bind(obs.effective_time)
    .bind(&obs.status)
    .bind(&obs.interpretation)
    .bind(source)
    .execute(pool)
    .await
    .map_err(OrganonError::Database)?;
    Ok(())
}

pub async fn insert_clinical_observations(
    pool: &PgPool,
    observations: &[ClinicalObservation],
    source: &str,
) -> Result<usize> {
    for obs in observations {
        insert_clinical_observation(pool, obs, source).await?;
    }
    Ok(observations.len())
}

/// Store the raw FHIR resource payload alongside the normalized observation row
/// (Keystone's Canopy JSON extension)
pub async fn insert_fhir_resource_payload(
    pool: &PgPool,
    resource_type: &str,
    resource_id: &str,
    payload: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO fhir_resource_payloads (resource_type, resource_id, payload)
        VALUES ($1, $2, $3)
        ON CONFLICT (resource_type, resource_id) DO UPDATE SET payload = EXCLUDED.payload
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(payload)
    .execute(pool)
    .await
    .map_err(OrganonError::Database)?;
    Ok(())
}

/// Insert organ imaging metadata (pixel data is expected to already be in MinIO)
pub async fn insert_organ_imaging_record(pool: &PgPool, record: &OrganImagingRecord) -> Result<()> {
    let laterality = record.laterality.as_ref().map(|l| format!("{:?}", l));

    sqlx::query(
        r#"
        INSERT INTO organ_imaging_records
            (id, subject_id, study_instance_uid, series_instance_uid, sop_instance_uid,
             modality, body_part_examined, organ_system, laterality,
             minio_bucket, minio_object_key, checksum_sha256, file_size_bytes, ingested_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (study_instance_uid, series_instance_uid, sop_instance_uid) DO NOTHING
        "#,
    )
    .bind(record.id)
    .bind(&record.subject_id)
    .bind(&record.dicom_metadata.study_instance_uid)
    .bind(&record.dicom_metadata.series_instance_uid)
    .bind(&record.dicom_metadata.sop_instance_uid)
    .bind(format!("{:?}", record.dicom_metadata.modality))
    .bind(&record.dicom_metadata.body_part_examined)
    .bind(&record.organ_system)
    .bind(laterality)
    .bind(&record.minio_bucket)
    .bind(&record.minio_object_key)
    .bind(&record.checksum_sha256)
    .bind(record.file_size_bytes as i64)
    .bind(record.ingested_at)
    .execute(pool)
    .await
    .map_err(OrganonError::Database)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_insert_clinical_observation() {
        // Would require a test database
    }
}
