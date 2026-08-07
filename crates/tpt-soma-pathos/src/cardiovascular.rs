//! Cardiovascular models (Phase 4, `tpt-soma-pathos`).
//!
//! A simplified ASCVD-style 10-year risk proxy. Illustrative and not for
//! clinical use; transparent so it can be audited.

use crate::{PathosError, Result};

/// Simplified ASCVD-style 10-year risk proxy in [0, 1] (illustrative).
///
/// Additive points model from standard risk factors; mapped through a logistic
/// function to a probability. Higher HDL is protective (negative points).
pub fn ascvd_10yr_risk_proxy(
    age: f64,
    is_female: bool,
    total_chol: f64,
    hdl: f64,
    sbp: f64,
    smoker: bool,
    diabetic: bool,
) -> Result<f64> {
    if age <= 0.0 || total_chol <= 0.0 || hdl <= 0.0 || sbp <= 0.0 {
        return Err(PathosError::InvalidInput(
            "age, total_chol, hdl, and sbp must be positive".to_string(),
        ));
    }
    let mut points = (age - 40.0).max(0.0) * 0.02;
    if is_female {
        points -= 2.0;
    }
    points += (total_chol - 180.0).max(0.0) / 40.0;
    points += ((hdl - 50.0).max(0.0) / 10.0) * -1.0;
    points += (sbp - 120.0).max(0.0) / 20.0;
    if smoker {
        points += 2.0;
    }
    if diabetic {
        points += 2.0;
    }
    let risk = 1.0 / (1.0 + (-(points) / 10.0).exp());
    Ok(risk.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_in_range_and_monotone_in_age() {
        let low = ascvd_10yr_risk_proxy(45.0, false, 190.0, 50.0, 125.0, false, false).unwrap();
        let high = ascvd_10yr_risk_proxy(70.0, false, 240.0, 40.0, 150.0, true, true).unwrap();
        assert!((0.0..=1.0).contains(&low));
        assert!((0.0..=1.0).contains(&high));
        assert!(high > low);
    }

    #[test]
    fn test_risk_rejects_nonpositive() {
        assert!(ascvd_10yr_risk_proxy(0.0, false, 190.0, 50.0, 125.0, false, false).is_err());
    }
}
