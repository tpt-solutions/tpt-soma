//! Trial protocol design: treatment arms and scheduled visits (Phase 4).
//!
//! Pure logic, no database access. Eligibility criteria stay in [`crate::trial`].
//! This module adds the design a protocol needs beyond them:
//!
//! - **Arms** with allocation ratios. Enrolled subjects are assigned to arms in
//!   a fixed cycle that follows the ratios. **This is not randomization.** The
//!   order of assignment follows screening order, so it can be predicted by
//!   anyone who knows that order. A trial that needs concealed or randomized
//!   allocation must not use this module as its allocation mechanism.
//! - **Visits** at day offsets from enrollment (day 0 is the enrollment day).

use chrono::{Days, NaiveDate};
use std::collections::HashSet;

use crate::{ClinicaError, Result};

/// A treatment arm. Its `allocation_ratio` sets its share of enrollments
/// relative to the other arms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm {
    pub name: String,
    pub allocation_ratio: u32,
}

/// A scheduled visit, `day` days after enrollment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub name: String,
    pub day: u32,
}

/// The arms and visits of one protocol. Empty for protocols created before
/// design was recorded. Those protocols get no arm on enrollment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtocolDesign {
    pub arms: Vec<Arm>,
    pub visits: Vec<Visit>,
}

impl ProtocolDesign {
    /// The arm for the `slot`-th subject who receives one (0-based). Slots
    /// cycle through the arms in proportion to their ratios, so with arms A (2)
    /// and B (1) the order is A, A, B, A, A, B, ... Returns `None` when there
    /// are no arms.
    pub fn arm_for_slot(&self, slot: usize) -> Option<&Arm> {
        let total: usize = self
            .arms
            .iter()
            .map(|arm| arm.allocation_ratio as usize)
            .sum();
        if total == 0 {
            return None;
        }
        let mut position = slot % total;
        for arm in &self.arms {
            let ratio = arm.allocation_ratio as usize;
            if position < ratio {
                return Some(arm);
            }
            position -= ratio;
        }
        None
    }
}

/// Check a design before it is stored. A protocol needs at least one arm and
/// one visit. Names must be non-empty and unique, ratios at least 1, and visit
/// days strictly increasing in the order given.
pub fn validate_design(design: &ProtocolDesign) -> Result<()> {
    if design.arms.is_empty() {
        return Err(invalid("a protocol needs at least one arm"));
    }
    if design.visits.is_empty() {
        return Err(invalid("a protocol needs at least one visit"));
    }

    let mut arm_names = HashSet::new();
    for arm in &design.arms {
        if arm.name.trim().is_empty() {
            return Err(invalid("arm names must not be empty"));
        }
        if !arm_names.insert(arm.name.as_str()) {
            return Err(invalid(format!("duplicate arm name {}", arm.name)));
        }
        if arm.allocation_ratio == 0 {
            return Err(invalid(format!(
                "arm {} needs an allocation ratio of at least 1",
                arm.name
            )));
        }
    }

    let mut visit_names = HashSet::new();
    let mut previous_day: Option<u32> = None;
    for visit in &design.visits {
        if visit.name.trim().is_empty() {
            return Err(invalid("visit names must not be empty"));
        }
        if !visit_names.insert(visit.name.as_str()) {
            return Err(invalid(format!("duplicate visit name {}", visit.name)));
        }
        if previous_day.is_some_and(|prev| visit.day <= prev) {
            return Err(invalid(format!(
                "visit {} must fall after the previous visit (day {})",
                visit.name, visit.day
            )));
        }
        previous_day = Some(visit.day);
    }
    Ok(())
}

/// Visit dates for a subject enrolled on `enrolled_on`, in protocol order.
pub fn visit_schedule(
    design: &ProtocolDesign,
    enrolled_on: NaiveDate,
) -> Result<Vec<(String, NaiveDate)>> {
    design
        .visits
        .iter()
        .map(|visit| {
            let date = enrolled_on
                .checked_add_days(Days::new(u64::from(visit.day)))
                .ok_or_else(|| invalid(format!("visit {} is out of range", visit.name)))?;
            Ok((visit.name.clone(), date))
        })
        .collect()
}

fn invalid(message: impl Into<String>) -> ClinicaError {
    ClinicaError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arm(name: &str, ratio: u32) -> Arm {
        Arm {
            name: name.to_string(),
            allocation_ratio: ratio,
        }
    }

    fn visit(name: &str, day: u32) -> Visit {
        Visit {
            name: name.to_string(),
            day,
        }
    }

    fn design(arms: Vec<Arm>, visits: Vec<Visit>) -> ProtocolDesign {
        ProtocolDesign { arms, visits }
    }

    #[test]
    fn arm_cycle_follows_allocation_ratio() {
        let d = design(vec![arm("A", 2), arm("B", 1)], vec![visit("baseline", 0)]);
        let order: Vec<&str> = (0..6)
            .map(|slot| d.arm_for_slot(slot).unwrap().name.as_str())
            .collect();
        assert_eq!(order, ["A", "A", "B", "A", "A", "B"]);
    }

    #[test]
    fn no_arms_gives_no_assignment() {
        let d = design(vec![], vec![visit("baseline", 0)]);
        assert!(d.arm_for_slot(0).is_none());
    }

    #[test]
    fn valid_design_is_accepted() {
        let d = design(
            vec![arm("treatment", 1), arm("control", 1)],
            vec![visit("baseline", 0), visit("week4", 28)],
        );
        assert!(validate_design(&d).is_ok());
    }

    #[test]
    fn design_without_arms_or_visits_is_rejected() {
        let no_arms = design(vec![], vec![visit("baseline", 0)]);
        assert!(validate_design(&no_arms).is_err());
        let no_visits = design(vec![arm("A", 1)], vec![]);
        assert!(validate_design(&no_visits).is_err());
    }

    #[test]
    fn duplicate_or_empty_arm_names_are_rejected() {
        let dup = design(vec![arm("A", 1), arm("A", 1)], vec![visit("baseline", 0)]);
        assert!(validate_design(&dup).is_err());
        let blank = design(vec![arm("  ", 1)], vec![visit("baseline", 0)]);
        assert!(validate_design(&blank).is_err());
    }

    #[test]
    fn zero_allocation_ratio_is_rejected() {
        let d = design(vec![arm("A", 0)], vec![visit("baseline", 0)]);
        assert!(validate_design(&d).is_err());
    }

    #[test]
    fn visit_days_must_strictly_increase() {
        let same_day = design(
            vec![arm("A", 1)],
            vec![visit("baseline", 0), visit("day0-again", 0)],
        );
        assert!(validate_design(&same_day).is_err());
        let backwards = design(
            vec![arm("A", 1)],
            vec![visit("week4", 28), visit("baseline", 0)],
        );
        assert!(validate_design(&backwards).is_err());
    }

    #[test]
    fn schedule_adds_day_offsets_to_enrollment_date() {
        let d = design(
            vec![arm("A", 1)],
            vec![visit("baseline", 0), visit("week4", 28)],
        );
        let enrolled = NaiveDate::from_ymd_opt(2026, 1, 30).unwrap();
        let schedule = visit_schedule(&d, enrolled).unwrap();
        assert_eq!(schedule[0], ("baseline".to_string(), enrolled));
        assert_eq!(
            schedule[1],
            (
                "week4".to_string(),
                NaiveDate::from_ymd_opt(2026, 2, 27).unwrap()
            )
        );
    }
}
