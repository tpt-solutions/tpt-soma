//! Persistence for clinica tables (Phase 4): clinical-trial cohorts and
//! biomarker-discovery results.

use crate::biomarker::BiomarkerResult;
use crate::{ClinicaError, Result};
use sqlx::PgPool;
use uuid::Uuid;

/// Record a clinical-trial cohort definition.
pub async fn insert_clinical_trial_cohort(
    pool: &PgPool,
    trial_name: &str,
    cohort_label: &str,
    inclusion: &[String],
    exclusion: &[String],
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO clinical_trial_cohorts
            (id, trial_name, cohort_label, inclusion_criteria, exclusion_criteria)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(id)
    .bind(trial_name)
    .bind(cohort_label)
    .bind(serde_json::to_value(inclusion).map_err(|e| ClinicaError::InvalidInput(e.to_string()))?)
    .bind(serde_json::to_value(exclusion).map_err(|e| ClinicaError::InvalidInput(e.to_string()))?)
    .execute(pool)
    .await
    .map_err(ClinicaError::Database)?;
    Ok(id)
}

/// Record a biomarker-association result.
pub async fn record_biomarker(
    pool: &PgPool,
    analysis_name: &str,
    biomarker: &str,
    result: &BiomarkerResult,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO biomarker_discovery
            (id, analysis_name, biomarker, statistic, p_value, result)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(analysis_name)
    .bind(biomarker)
    .bind(result.statistic)
    .bind(result.p_value)
    .bind(serde_json::json!({ "effect_size": result.effect_size }))
    .execute(pool)
    .await
    .map_err(ClinicaError::Database)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biomarker_result_struct() {
        let r = BiomarkerResult {
            statistic: 3.2,
            p_value: 0.001,
            effect_size: 1.1,
        };
        assert_eq!(r.statistic, 3.2);
        assert!(r.p_value < 0.01);
    }
}
