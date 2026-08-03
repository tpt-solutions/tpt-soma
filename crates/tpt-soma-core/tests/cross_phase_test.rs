use chrono::Utc;
use tpt_soma_core::connection::{create_pool, run_migrations};
use tpt_soma_core::query::get_cross_phase_subject_summary;
use uuid::Uuid;

/// Cross-phase integration test: a sample linked across Phase 1 genomic/cytos
/// records (via `samples.patient_id`) and Phase 2 clinical/CGM records (via
/// `subject_id`), reachable through a single combined query.
#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_cross_phase_subject_summary() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/tpt_soma_test".to_string()
    });
    let pool = create_pool(&database_url).await?;
    run_migrations(&pool).await?;

    let patient_id = Uuid::new_v4();
    let patient_id_str = patient_id.to_string();

    // Phase 1: a sample linked to this patient
    sqlx::query("INSERT INTO samples (patient_id, source) VALUES ($1, 'patient')")
        .bind(patient_id)
        .execute(&pool)
        .await?;

    // Phase 2: a clinical observation and a CGM reading for the same subject
    sqlx::query(
        r#"
        INSERT INTO organ_function_observations (subject_id, loinc_code, value, effective_time)
        VALUES ($1, '2160-0', 1.1, now())
        "#,
    )
    .bind(&patient_id_str)
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO cgm_readings (subject_id, ts, glucose_mgdl, source)
        VALUES ($1, $2, 105.0, 'DexcomG6')
        "#,
    )
    .bind(&patient_id_str)
    .bind(Utc::now())
    .execute(&pool)
    .await?;

    let summary = get_cross_phase_subject_summary(&pool, &patient_id_str).await?;

    assert_eq!(summary.phase1_sample_count, 1);
    assert_eq!(summary.phase2_observation_count, 1);
    assert_eq!(summary.phase2_cgm_reading_count, 1);

    Ok(())
}
