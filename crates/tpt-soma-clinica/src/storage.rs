//! Persistence for clinica tables (Phase 4): clinical-trial cohorts and
//! biomarker-discovery results.

use crate::biomarker::BiomarkerResult;
use crate::protocol::{Arm, ProtocolDesign, Visit, validate_design};
use crate::trial::{
    AdverseEvent, AeSummary, Criterion, SubjectFeatures, evaluate_eligibility,
    summarize_adverse_events,
};
use crate::{ClinicaError, Result};
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
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

/// Persist a protocol with its arms and visits as well as its criteria.
pub async fn save_trial_protocol_design(
    pool: &PgPool,
    trial_name: &str,
    cohort_label: &str,
    criteria: &[Criterion],
    design: &ProtocolDesign,
) -> Result<Uuid> {
    validate_design(design)?;
    evaluate_eligibility(&SubjectFeatures::new(), criteria)?;
    let (inc, exc) = criteria_to_json(criteria);
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO clinical_trial_cohorts
            (id, trial_name, cohort_label, inclusion_criteria, exclusion_criteria, arms, visits)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(id)
    .bind(trial_name)
    .bind(cohort_label)
    .bind(inc)
    .bind(exc)
    .bind(arms_to_json(&design.arms))
    .bind(visits_to_json(&design.visits))
    .execute(pool)
    .await
    .map_err(ClinicaError::Database)?;
    Ok(id)
}

fn arms_to_json(arms: &[Arm]) -> serde_json::Value {
    arms.iter()
        .map(
            |arm| serde_json::json!({ "name": arm.name, "allocation_ratio": arm.allocation_ratio }),
        )
        .collect::<Vec<_>>()
        .into()
}

fn visits_to_json(visits: &[Visit]) -> serde_json::Value {
    visits
        .iter()
        .map(|visit| serde_json::json!({ "name": visit.name, "day": visit.day }))
        .collect::<Vec<_>>()
        .into()
}

fn malformed(what: &str) -> ClinicaError {
    ClinicaError::InvalidInput(format!("malformed {what} JSON"))
}

/// Load the arms and visits saved by [`save_trial_protocol_design`]. A protocol
/// saved without a design returns an empty one.
pub async fn load_trial_design(pool: &PgPool, trial_cohort_id: Uuid) -> Result<ProtocolDesign> {
    let row = sqlx::query("SELECT arms, visits FROM clinical_trial_cohorts WHERE id = $1")
        .bind(trial_cohort_id)
        .fetch_optional(pool)
        .await
        .map_err(ClinicaError::Database)?
        .ok_or_else(|| {
            ClinicaError::InvalidInput(format!("unknown trial cohort {trial_cohort_id}"))
        })?;

    let arms = row
        .get::<serde_json::Value, _>("arms")
        .as_array()
        .ok_or_else(|| malformed("arm"))?
        .iter()
        .map(|o| -> Result<Arm> {
            Ok(Arm {
                name: o["name"]
                    .as_str()
                    .ok_or_else(|| malformed("arm"))?
                    .to_string(),
                allocation_ratio: o["allocation_ratio"]
                    .as_u64()
                    .and_then(|r| u32::try_from(r).ok())
                    .ok_or_else(|| malformed("arm"))?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let visits = row
        .get::<serde_json::Value, _>("visits")
        .as_array()
        .ok_or_else(|| malformed("visit"))?
        .iter()
        .map(|o| -> Result<Visit> {
            Ok(Visit {
                name: o["name"]
                    .as_str()
                    .ok_or_else(|| malformed("visit"))?
                    .to_string(),
                day: o["day"]
                    .as_u64()
                    .and_then(|d| u32::try_from(d).ok())
                    .ok_or_else(|| malformed("visit"))?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ProtocolDesign { arms, visits })
}

/// An enrolled subject's arm and the date of the enrollment decision, which is
/// day 0 for the visit schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectEnrollment {
    /// `None` for subjects enrolled under a protocol without arms.
    pub arm: Option<String>,
    pub enrolled_on: NaiveDate,
}

/// Look up an enrolled subject. Returns `None` if the subject is not enrolled.
pub async fn load_subject_enrollment(
    pool: &PgPool,
    trial_cohort_id: Uuid,
    subject_id: &str,
) -> Result<Option<SubjectEnrollment>> {
    let row = sqlx::query(
        r#"
        SELECT arm, screened_at FROM trial_enrollments
        WHERE trial_cohort_id = $1 AND subject_id = $2 AND status = 'enrolled'
        "#,
    )
    .bind(trial_cohort_id)
    .bind(subject_id)
    .fetch_optional(pool)
    .await
    .map_err(ClinicaError::Database)?;
    Ok(row.map(|row| SubjectEnrollment {
        arm: row.get::<Option<String>, _>("arm"),
        enrolled_on: row.get::<DateTime<Utc>, _>("screened_at").date_naive(),
    }))
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
///
/// `target` is the total number of enrolled subjects for the trial, not a
/// per-call quota. Subjects already enrolled count toward it, and they are left
/// as they are: a re-screen does not de-enroll anyone, and keeps their arm.
/// Ineligible candidates are recorded as `screen_failed` with the criteria they
/// failed. Eligible candidates beyond the target are left unrecorded so they can
/// be screened again later. Returns the number of subjects enrolled after the
/// call.
///
/// The trial row is locked for the whole screening, so two concurrent screens
/// cannot both take the last place or the same arm slot.
pub async fn screen_and_persist(
    pool: &PgPool,
    trial_cohort_id: Uuid,
    candidates: &[(String, SubjectFeatures)],
    target: usize,
) -> Result<usize> {
    let criteria = load_trial_criteria(pool, trial_cohort_id).await?;
    let design = load_trial_design(pool, trial_cohort_id).await?;
    let mut tx = pool.begin().await.map_err(ClinicaError::Database)?;

    sqlx::query("SELECT id FROM clinical_trial_cohorts WHERE id = $1 FOR UPDATE")
        .bind(trial_cohort_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ClinicaError::Database)?;

    // Current state of every recorded subject. A subject keeps the arm it was
    // first given, and new arms continue the cycle from the number of arms
    // already assigned.
    let rows = sqlx::query(
        "SELECT subject_id, status, arm FROM trial_enrollments WHERE trial_cohort_id = $1",
    )
    .bind(trial_cohort_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ClinicaError::Database)?;

    let mut enrolled_subjects: HashSet<String> = HashSet::new();
    let mut arms: HashMap<String, Option<String>> = HashMap::new();
    let mut next_slot = 0usize;
    for row in &rows {
        let subject: String = row.get("subject_id");
        let arm: Option<String> = row.get("arm");
        if row.get::<String, _>("status") == "enrolled" {
            enrolled_subjects.insert(subject.clone());
        }
        if arm.is_some() {
            next_slot += 1;
        }
        arms.insert(subject, arm);
    }
    let mut enrolled = enrolled_subjects.len();

    for (subject_id, features) in candidates {
        // Already enrolled: nothing to decide, and nobody is removed by a re-screen.
        if enrolled_subjects.contains(subject_id) {
            continue;
        }
        let result = evaluate_eligibility(features, &criteria)?;
        let (status, arm) = if result.eligible {
            if enrolled >= target {
                continue;
            }
            enrolled += 1;
            let arm = match arms.get(subject_id) {
                Some(Some(arm)) => Some(arm.clone()),
                _ => match design.arm_for_slot(next_slot) {
                    Some(arm) => {
                        next_slot += 1;
                        Some(arm.name.clone())
                    }
                    None => None,
                },
            };
            ("enrolled", arm)
        } else {
            ("screen_failed", None)
        };
        sqlx::query(
            r#"
            INSERT INTO trial_enrollments
                (trial_cohort_id, subject_id, status, failed_criteria, arm)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (trial_cohort_id, subject_id) DO UPDATE
                SET status = EXCLUDED.status,
                    failed_criteria = EXCLUDED.failed_criteria,
                    arm = COALESCE(trial_enrollments.arm, EXCLUDED.arm),
                    screened_at = now()
            "#,
        )
        .bind(trial_cohort_id)
        .bind(subject_id)
        .bind(status)
        .bind(serde_json::json!(result.failed))
        .bind(arm)
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

    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_protocol_design_assigns_arms_and_keeps_them() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let criteria = vec![Criterion::Inclusion {
            key: "hba1c".to_string(),
            min: 6.5,
            max: 10.0,
        }];
        let design = ProtocolDesign {
            arms: vec![
                Arm {
                    name: "treatment".to_string(),
                    allocation_ratio: 2,
                },
                Arm {
                    name: "control".to_string(),
                    allocation_ratio: 1,
                },
            ],
            visits: vec![
                Visit {
                    name: "baseline".to_string(),
                    day: 0,
                },
                Visit {
                    name: "week4".to_string(),
                    day: 28,
                },
            ],
        };
        let id = save_trial_protocol_design(&pool, "T2", "arm-b", &criteria, &design)
            .await
            .unwrap();
        assert_eq!(load_trial_design(&pool, id).await.unwrap(), design);

        // Every candidate is eligible. Slots follow the 2:1 cycle A, A, B, A, ...
        let feat = SubjectFeatures::from([("hba1c".to_string(), 7.0)]);
        let candidates: Vec<(String, SubjectFeatures)> =
            (1..=6).map(|i| (format!("p{i}"), feat.clone())).collect();
        assert_eq!(
            screen_and_persist(&pool, id, &candidates, 4).await.unwrap(),
            4
        );
        let arm_of = |subject: &'static str| {
            let pool = pool.clone();
            async move {
                load_subject_enrollment(&pool, id, subject)
                    .await
                    .unwrap()
                    .and_then(|e| e.arm)
            }
        };
        assert_eq!(arm_of("p1").await.as_deref(), Some("treatment"));
        assert_eq!(arm_of("p2").await.as_deref(), Some("treatment"));
        assert_eq!(arm_of("p3").await.as_deref(), Some("control"));
        assert_eq!(arm_of("p4").await.as_deref(), Some("treatment"));
        assert_eq!(arm_of("p5").await, None, "p5 was beyond the target");

        // Re-screening with a larger target keeps existing arms and continues the cycle.
        assert_eq!(
            screen_and_persist(&pool, id, &candidates, 5).await.unwrap(),
            5
        );
        assert_eq!(arm_of("p1").await.as_deref(), Some("treatment"));
        assert_eq!(arm_of("p3").await.as_deref(), Some("control"));
        assert_eq!(arm_of("p5").await.as_deref(), Some("treatment"));

        let enrollment = load_subject_enrollment(&pool, id, "p3")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(enrollment.arm.as_deref(), Some("control"));
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_rescreen_counts_existing_enrollments_toward_target() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let criteria = vec![Criterion::Inclusion {
            key: "hba1c".to_string(),
            min: 6.5,
            max: 10.0,
        }];
        let id = save_trial_protocol(&pool, "T5", "overenroll", &criteria)
            .await
            .unwrap();
        let eligible = |name: &str| {
            (
                name.to_string(),
                SubjectFeatures::from([("hba1c".to_string(), 7.0)]),
            )
        };

        let first = vec![eligible("q1"), eligible("q2")];
        assert_eq!(screen_and_persist(&pool, id, &first, 2).await.unwrap(), 2);

        // Target 2 is already met. q3 comes first, so the old per-call counter
        // would have enrolled it. It must not be enrolled now.
        let second = vec![eligible("q3"), eligible("q1"), eligible("q2")];
        assert_eq!(screen_and_persist(&pool, id, &second, 2).await.unwrap(), 2);
        assert!(
            load_subject_enrollment(&pool, id, "q3")
                .await
                .unwrap()
                .is_none()
        );

        // A re-screen where q1 now fails criteria does not de-enroll q1.
        let failing = vec![(
            "q1".to_string(),
            SubjectFeatures::from([("hba1c".to_string(), 4.0)]),
        )];
        assert_eq!(screen_and_persist(&pool, id, &failing, 2).await.unwrap(), 2);
        let q1 = load_subject_enrollment(&pool, id, "q1").await.unwrap();
        assert!(
            q1.is_some(),
            "q1 must stay enrolled after a failing re-screen"
        );
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_protocol_without_design_enrolls_without_arm() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let criteria = vec![Criterion::Inclusion {
            key: "hba1c".to_string(),
            min: 6.5,
            max: 10.0,
        }];
        let id = save_trial_protocol(&pool, "T3", "legacy", &criteria)
            .await
            .unwrap();
        assert_eq!(
            load_trial_design(&pool, id).await.unwrap(),
            ProtocolDesign::default()
        );

        let candidates = vec![(
            "legacy-1".to_string(),
            SubjectFeatures::from([("hba1c".to_string(), 7.0)]),
        )];
        assert_eq!(
            screen_and_persist(&pool, id, &candidates, 1).await.unwrap(),
            1
        );
        let enrollment = load_subject_enrollment(&pool, id, "legacy-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(enrollment.arm, None);
    }

    #[tokio::test]
    #[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
    async fn test_invalid_design_writes_nothing() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let design = ProtocolDesign {
            arms: vec![],
            visits: vec![Visit {
                name: "baseline".to_string(),
                day: 0,
            }],
        };
        assert!(
            save_trial_protocol_design(&pool, "T4", "bad", &[], &design)
                .await
                .is_err()
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM clinical_trial_cohorts WHERE trial_name = 'T4'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
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
