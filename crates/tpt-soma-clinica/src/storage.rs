//! Persistence for clinica tables (Phase 4): clinical-trial cohorts and
//! biomarker-discovery results.

use crate::biomarker::BiomarkerResult;
use crate::trial::{
    AdverseEvent, AeSummary, Criterion, SubjectFeatures, evaluate_eligibility,
    summarize_adverse_events,
};
use crate::{ClinicaError, Result};
use sqlx::{PgPool, Row};
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

/// Encode criteria as `(inclusion, exclusion)` JSON arrays of `{key,min,max}`.
fn criteria_to_json(criteria: &[Criterion]) -> (serde_json::Value, serde_json::Value) {
    let (mut inc, mut exc) = (Vec::new(), Vec::new());
    for c in criteria {
        match c {
            Criterion::Inclusion { key, min, max } => {
                inc.push(serde_json::json!({ "key": key, "min": min, "max": max }));
            }
            Criterion::Exclusion { key, min, max } => {
                exc.push(serde_json::json!({ "key": key, "min": min, "max": max }));
            }
        }
    }
    (inc.into(), exc.into())
}

fn criteria_from_json(
    inclusion: &serde_json::Value,
    exclusion: &serde_json::Value,
) -> Result<Vec<Criterion>> {
    fn parse(v: &serde_json::Value) -> Result<Vec<(String, f64, f64)>> {
        let bad = || ClinicaError::InvalidInput("malformed criterion JSON".to_string());
        v.as_array()
            .ok_or_else(bad)?
            .iter()
            .map(|o| {
                Ok((
                    o["key"].as_str().ok_or_else(bad)?.to_string(),
                    o["min"].as_f64().ok_or_else(bad)?,
                    o["max"].as_f64().ok_or_else(bad)?,
                ))
            })
            .collect()
    }
    let mut out = Vec::new();
    for (key, min, max) in parse(inclusion)? {
        out.push(Criterion::Inclusion { key, min, max });
    }
    for (key, min, max) in parse(exclusion)? {
        out.push(Criterion::Exclusion { key, min, max });
    }
    Ok(out)
}

/// Persist a trial protocol's eligibility criteria as a trial cohort.
pub async fn save_trial_protocol(
    pool: &PgPool,
    trial_name: &str,
    cohort_label: &str,
    criteria: &[Criterion],
) -> Result<Uuid> {
    // Reject inverted bounds before anything is written.
    evaluate_eligibility(&SubjectFeatures::new(), criteria)?;
    let (inc, exc) = criteria_to_json(criteria);
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
    .bind(inc)
    .bind(exc)
    .execute(pool)
    .await
    .map_err(ClinicaError::Database)?;
    Ok(id)
}

/// Load the criteria saved by [`save_trial_protocol`].
pub async fn load_trial_criteria(pool: &PgPool, trial_cohort_id: Uuid) -> Result<Vec<Criterion>> {
    let row = sqlx::query(
        "SELECT inclusion_criteria, exclusion_criteria FROM clinical_trial_cohorts WHERE id = $1",
    )
    .bind(trial_cohort_id)
    .fetch_optional(pool)
    .await
    .map_err(ClinicaError::Database)?
    .ok_or_else(|| ClinicaError::InvalidInput(format!("unknown trial cohort {trial_cohort_id}")))?;
    criteria_from_json(
        &row.get::<serde_json::Value, _>("inclusion_criteria"),
        &row.get::<serde_json::Value, _>("exclusion_criteria"),
    )
}

/// Screen candidates against the cohort's saved criteria and record decisions.
/// The first `target` eligible subjects are `enrolled`; ineligible candidates
/// are recorded as `screen_failed` with the criteria they failed. Eligible
/// subjects beyond the target are left unrecorded so they can be screened
/// again later. Returns the number of subjects enrolled.
pub async fn screen_and_persist(
    pool: &PgPool,
    trial_cohort_id: Uuid,
    candidates: &[(String, SubjectFeatures)],
    target: usize,
) -> Result<usize> {
    let criteria = load_trial_criteria(pool, trial_cohort_id).await?;
    let mut tx = pool.begin().await.map_err(ClinicaError::Database)?;
    let mut enrolled = 0usize;
    for (subject_id, features) in candidates {
        let result = evaluate_eligibility(features, &criteria)?;
        let status = if result.eligible {
            if enrolled >= target {
                continue;
            }
            enrolled += 1;
            "enrolled"
        } else {
            "screen_failed"
        };
        sqlx::query(
            r#"
            INSERT INTO trial_enrollments
                (trial_cohort_id, subject_id, status, failed_criteria)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (trial_cohort_id, subject_id) DO UPDATE
                SET status = EXCLUDED.status,
                    failed_criteria = EXCLUDED.failed_criteria,
                    screened_at = now()
            "#,
        )
        .bind(trial_cohort_id)
        .bind(subject_id)
        .bind(status)
        .bind(serde_json::json!(result.failed))
        .execute(&mut *tx)
        .await
        .map_err(ClinicaError::Database)?;
    }
    tx.commit().await.map_err(ClinicaError::Database)?;
    Ok(enrolled)
}

/// Record adverse events for a trial cohort. Grades are validated first, so an
/// invalid event writes nothing.
pub async fn record_adverse_events(
    pool: &PgPool,
    trial_cohort_id: Uuid,
    events: &[AdverseEvent],
) -> Result<usize> {
    summarize_adverse_events(events)?;
    let mut tx = pool.begin().await.map_err(ClinicaError::Database)?;
    for ev in events {
        sqlx::query(
            r#"
            INSERT INTO trial_adverse_events
                (trial_cohort_id, subject_id, term, grade, serious)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(trial_cohort_id)
        .bind(&ev.subject_id)
        .bind(&ev.term)
        .bind(i16::from(ev.grade))
        .bind(ev.serious)
        .execute(&mut *tx)
        .await
        .map_err(ClinicaError::Database)?;
    }
    tx.commit().await.map_err(ClinicaError::Database)?;
    Ok(events.len())
}

/// Summarize the persisted adverse events of a trial cohort.
pub async fn load_ae_summary(pool: &PgPool, trial_cohort_id: Uuid) -> Result<AeSummary> {
    let rows = sqlx::query(
        "SELECT subject_id, term, grade, serious FROM trial_adverse_events WHERE trial_cohort_id = $1",
    )
    .bind(trial_cohort_id)
    .fetch_all(pool)
    .await
    .map_err(ClinicaError::Database)?;
    let events: Vec<AdverseEvent> = rows
        .iter()
        .map(|r| AdverseEvent {
            subject_id: r.get("subject_id"),
            term: r.get("term"),
            grade: u8::try_from(r.get::<i16, _>("grade")).unwrap_or(0),
            serious: r.get("serious"),
        })
        .collect();
    summarize_adverse_events(&events)
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
    fn test_criteria_json_round_trip() {
        let criteria = vec![
            Criterion::Inclusion {
                key: "hba1c".to_string(),
                min: 6.5,
                max: 10.0,
            },
            Criterion::Exclusion {
                key: "egfr".to_string(),
                min: 0.0,
                max: 30.0,
            },
        ];
        let (inc, exc) = criteria_to_json(&criteria);
        assert_eq!(criteria_from_json(&inc, &exc).unwrap(), criteria);
    }

    #[test]
    fn test_criteria_from_malformed_json_is_rejected() {
        let bad = serde_json::json!([{ "key": "x" }]);
        assert!(criteria_from_json(&bad, &serde_json::json!([])).is_err());
        assert!(criteria_from_json(&serde_json::json!({}), &serde_json::json!([])).is_err());
    }

    async fn test_pool() -> Option<PgPool> {
        match std::env::var("TEST_DATABASE_URL") {
            Ok(url) => Some(PgPool::connect(&url).await.unwrap()),
            Err(_) => None,
        }
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_screen_persist_and_ae_summary() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let criteria = vec![Criterion::Inclusion {
            key: "hba1c".to_string(),
            min: 6.5,
            max: 10.0,
        }];
        let id = save_trial_protocol(&pool, "T1", "arm-a", &criteria)
            .await
            .unwrap();
        assert_eq!(load_trial_criteria(&pool, id).await.unwrap(), criteria);

        let feat = |v: f64| SubjectFeatures::from([("hba1c".to_string(), v)]);
        let candidates = vec![
            ("s1".to_string(), feat(5.0)),
            ("s2".to_string(), feat(7.0)),
            ("s3".to_string(), feat(8.0)),
        ];
        assert_eq!(
            screen_and_persist(&pool, id, &candidates, 1).await.unwrap(),
            1
        );

        let events = vec![AdverseEvent {
            subject_id: "s2".to_string(),
            term: "nausea".to_string(),
            grade: 3,
            serious: true,
        }];
        record_adverse_events(&pool, id, &events).await.unwrap();
        let summary = load_ae_summary(&pool, id).await.unwrap();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.grade_3_or_higher, 1);
    }

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
