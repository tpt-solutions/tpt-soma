//! Clinical trial management logic (Phase 4, `tpt-soma-clinica`).
//!
//! Pure, database-free logic: adverse-event tracking summaries, inclusion and
//! exclusion criteria evaluation over subject features, and screening that
//! enrolls the first eligible subjects up to a target. Persistence onto the
//! `clinical_trial_cohorts` tables is layered on top of these functions.

use std::collections::{HashMap, HashSet};

use crate::{ClinicaError, Result};

/// One adverse event for a trial subject. `grade` follows CTCAE-style severity:
/// 1 (mild) through 5 (death related to the event).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdverseEvent {
    pub subject_id: String,
    pub term: String,
    pub grade: u8,
    pub serious: bool,
}

/// Aggregate adverse-event counts for a trial.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AeSummary {
    pub total: usize,
    /// Counts indexed by grade - 1 (`by_grade[0]` is grade 1).
    pub by_grade: [usize; 5],
    pub grade_3_or_higher: usize,
    pub serious: usize,
    pub subjects_with_ae: usize,
}

/// Summarize adverse events. Fails if any grade is outside 1..=5.
pub fn summarize_adverse_events(events: &[AdverseEvent]) -> Result<AeSummary> {
    let mut summary = AeSummary::default();
    let mut subjects = HashSet::new();
    for ev in events {
        if !(1..=5).contains(&ev.grade) {
            return Err(ClinicaError::InvalidInput(format!(
                "adverse event grade must be in 1..=5, got {}",
                ev.grade
            )));
        }
        summary.total += 1;
        summary.by_grade[usize::from(ev.grade) - 1] += 1;
        if ev.grade >= 3 {
            summary.grade_3_or_higher += 1;
        }
        if ev.serious {
            summary.serious += 1;
        }
        subjects.insert(ev.subject_id.as_str());
    }
    summary.subjects_with_ae = subjects.len();
    Ok(summary)
}

/// Subject features keyed by variable name (a LOINC code or a trial variable).
pub type SubjectFeatures = HashMap<String, f64>;

/// A protocol eligibility criterion over a single feature.
#[derive(Debug, Clone, PartialEq)]
pub enum Criterion {
    /// Subject must have `key` with a value in `[min, max]`. A missing value fails.
    Inclusion { key: String, min: f64, max: f64 },
    /// Subject is excluded when `key` is present with a value in `[min, max]`.
    /// A missing value does not exclude.
    Exclusion { key: String, min: f64, max: f64 },
}

impl Criterion {
    fn bounds(&self) -> (&str, f64, f64) {
        match self {
            Self::Inclusion { key, min, max } | Self::Exclusion { key, min, max } => {
                (key.as_str(), *min, *max)
            }
        }
    }
}

/// Outcome of evaluating one subject against a set of criteria.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EligibilityResult {
    pub eligible: bool,
    /// Human-readable labels of the criteria that failed, e.g. `inclusion:hba1c`.
    pub failed: Vec<String>,
}

fn validate_criteria(criteria: &[Criterion]) -> Result<()> {
    for c in criteria {
        let (key, min, max) = c.bounds();
        if min > max {
            return Err(ClinicaError::InvalidInput(format!(
                "criterion {key} has min {min} greater than max {max}"
            )));
        }
    }
    Ok(())
}

/// Evaluate one subject's features against the criteria.
pub fn evaluate_eligibility(
    features: &SubjectFeatures,
    criteria: &[Criterion],
) -> Result<EligibilityResult> {
    validate_criteria(criteria)?;
    let mut failed = Vec::new();
    for c in criteria {
        let (key, min, max) = c.bounds();
        let value = features.get(key).copied();
        match c {
            Criterion::Inclusion { .. } => {
                if !value.is_some_and(|v| (min..=max).contains(&v)) {
                    failed.push(format!("inclusion:{key}"));
                }
            }
            Criterion::Exclusion { .. } => {
                if value.is_some_and(|v| (min..=max).contains(&v)) {
                    failed.push(format!("exclusion:{key}"));
                }
            }
        }
    }
    Ok(EligibilityResult {
        eligible: failed.is_empty(),
        failed,
    })
}

/// Result of screening a candidate pool against a trial's criteria.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitmentReport {
    pub screened: usize,
    /// Total number of subjects that met every criterion.
    pub eligible: usize,
    /// First `target` eligible subject ids, in screening order.
    pub enrolled: Vec<String>,
    /// How many enrollments are still missing against the target.
    pub shortfall: usize,
}

/// Screen candidates in order and enroll the first `target` eligible subjects.
pub fn screen_and_recruit(
    candidates: &[(String, SubjectFeatures)],
    criteria: &[Criterion],
    target: usize,
) -> Result<RecruitmentReport> {
    validate_criteria(criteria)?;
    let mut eligible = 0usize;
    let mut enrolled = Vec::new();
    for (subject_id, features) in candidates {
        if evaluate_eligibility(features, criteria)?.eligible {
            eligible += 1;
            if enrolled.len() < target {
                enrolled.push(subject_id.clone());
            }
        }
    }
    let shortfall = target.saturating_sub(enrolled.len());
    Ok(RecruitmentReport {
        screened: candidates.len(),
        eligible,
        enrolled,
        shortfall,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ae(subject: &str, grade: u8, serious: bool) -> AdverseEvent {
        AdverseEvent {
            subject_id: subject.to_string(),
            term: "nausea".to_string(),
            grade,
            serious,
        }
    }

    fn features(pairs: &[(&str, f64)]) -> SubjectFeatures {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn hba1c_window() -> Vec<Criterion> {
        vec![
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
        ]
    }

    #[test]
    fn test_ae_summary_counts_grades_seriousness_and_subjects() {
        let events = vec![
            ae("s1", 1, false),
            ae("s1", 3, true),
            ae("s2", 2, false),
            ae("s3", 5, true),
        ];
        let s = summarize_adverse_events(&events).unwrap();
        assert_eq!(s.total, 4);
        assert_eq!(s.by_grade, [1, 1, 1, 0, 1]);
        assert_eq!(s.grade_3_or_higher, 2);
        assert_eq!(s.serious, 2);
        assert_eq!(s.subjects_with_ae, 3);
    }

    #[test]
    fn test_ae_summary_rejects_invalid_grade() {
        assert!(summarize_adverse_events(&[ae("s1", 0, false)]).is_err());
        assert!(summarize_adverse_events(&[ae("s1", 6, false)]).is_err());
    }

    #[test]
    fn test_eligibility_inclusion_and_exclusion() {
        let criteria = hba1c_window();
        let ok =
            evaluate_eligibility(&features(&[("hba1c", 7.5), ("egfr", 80.0)]), &criteria).unwrap();
        assert!(ok.eligible);
        assert!(ok.failed.is_empty());

        let excluded =
            evaluate_eligibility(&features(&[("hba1c", 7.5), ("egfr", 20.0)]), &criteria).unwrap();
        assert!(!excluded.eligible);
        assert_eq!(excluded.failed, vec!["exclusion:egfr".to_string()]);
    }

    #[test]
    fn test_missing_inclusion_fails_and_missing_exclusion_passes() {
        let criteria = hba1c_window();
        let no_hba1c = evaluate_eligibility(&features(&[("egfr", 80.0)]), &criteria).unwrap();
        assert_eq!(no_hba1c.failed, vec!["inclusion:hba1c".to_string()]);

        let no_egfr = evaluate_eligibility(&features(&[("hba1c", 7.5)]), &criteria).unwrap();
        assert!(no_egfr.eligible);
    }

    #[test]
    fn test_criteria_with_inverted_bounds_are_rejected() {
        let bad = vec![Criterion::Inclusion {
            key: "hba1c".to_string(),
            min: 10.0,
            max: 6.5,
        }];
        assert!(evaluate_eligibility(&features(&[("hba1c", 7.0)]), &bad).is_err());
    }

    #[test]
    fn test_screen_and_recruit_enrolls_first_eligible_up_to_target() {
        let criteria = hba1c_window();
        let candidates = vec![
            (
                "s1".to_string(),
                features(&[("hba1c", 5.0), ("egfr", 80.0)]),
            ),
            (
                "s2".to_string(),
                features(&[("hba1c", 7.0), ("egfr", 80.0)]),
            ),
            (
                "s3".to_string(),
                features(&[("hba1c", 8.0), ("egfr", 10.0)]),
            ),
            (
                "s4".to_string(),
                features(&[("hba1c", 9.0), ("egfr", 60.0)]),
            ),
            (
                "s5".to_string(),
                features(&[("hba1c", 8.5), ("egfr", 90.0)]),
            ),
        ];
        let report = screen_and_recruit(&candidates, &criteria, 2).unwrap();
        assert_eq!(report.screened, 5);
        assert_eq!(report.eligible, 3);
        assert_eq!(report.enrolled, vec!["s2".to_string(), "s4".to_string()]);
        assert_eq!(report.shortfall, 0);
    }

    #[test]
    fn test_screen_and_recruit_reports_shortfall() {
        let criteria = hba1c_window();
        let candidates = vec![("s1".to_string(), features(&[("hba1c", 7.0)]))];
        let report = screen_and_recruit(&candidates, &criteria, 3).unwrap();
        assert_eq!(report.eligible, 1);
        assert_eq!(report.enrolled.len(), 1);
        assert_eq!(report.shortfall, 2);
    }
}
