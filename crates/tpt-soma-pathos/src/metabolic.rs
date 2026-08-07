//! Metabolic & endocrine (diabetes) pathology models (Phase 4, `tpt-soma-pathos`).
//!
//! These build directly on Phase 2 data: fasting glucose / insulin (clinical
//! observations) and time-in-range (from `tpt-soma-chronos` CGM analytics).

use crate::{PathosError, Result};

/// Homeostatic Model Assessment of Insulin Resistance (HOMA-IR).
///
/// `glucose_mgdl` is fasting plasma glucose in mg/dL and `insulin_uiuml` is
/// fasting insulin in µIU/mL. Formula uses the common 405 denominator.
pub fn homa_ir(glucose_mgdl: f64, insulin_uiuml: f64) -> Result<f64> {
    if glucose_mgdl <= 0.0 || insulin_uiuml <= 0.0 {
        return Err(PathosError::InvalidInput(
            "fasting glucose and insulin must be positive".to_string(),
        ));
    }
    Ok((glucose_mgdl * insulin_uiuml) / 405.0)
}

/// Patient metrics for metabolic syndrome classification (NCEP ATP III).
#[derive(Debug, Clone)]
pub struct MetabolicSyndromeInput {
    pub waist_cm: f64,
    pub is_female: bool,
    pub triglycerides_mgdl: f64,
    pub hdl_mgdl: f64,
    pub systolic_bp: f64,
    pub diastolic_bp: f64,
    pub fasting_glucose_mgdl: f64,
    pub on_bp_meds: bool,
    pub on_glucose_meds: bool,
}

/// Result of `metabolic_syndrome_assessment`.
#[derive(Debug, Clone)]
pub struct MetabolicSyndromeResult {
    pub criteria_met: usize,
    pub is_metabolic_syndrome: bool,
    pub waist: bool,
    pub triglycerides: bool,
    pub hdl: bool,
    pub blood_pressure: bool,
    pub glucose: bool,
}

/// NCEP ATP III: ≥ 3 of 5 criteria satisfy metabolic syndrome.
pub fn metabolic_syndrome_assessment(
    input: &MetabolicSyndromeInput,
) -> Result<MetabolicSyndromeResult> {
    let waist = if input.is_female {
        input.waist_cm >= 88.0
    } else {
        input.waist_cm >= 102.0
    };
    let triglycerides = input.triglycerides_mgdl >= 150.0;
    let hdl = if input.is_female {
        input.hdl_mgdl < 50.0
    } else {
        input.hdl_mgdl < 40.0
    };
    let blood_pressure =
        input.on_bp_meds || input.systolic_bp >= 130.0 || input.diastolic_bp >= 85.0;
    let glucose = input.on_glucose_meds || input.fasting_glucose_mgdl >= 100.0;

    let criteria_met = [waist, triglycerides, hdl, blood_pressure, glucose]
        .iter()
        .filter(|x| **x)
        .count();
    Ok(MetabolicSyndromeResult {
        criteria_met,
        is_metabolic_syndrome: criteria_met >= 3,
        waist,
        triglycerides,
        hdl,
        blood_pressure,
        glucose,
    })
}

/// Combined insulin-resistance risk score in [0, 100].
///
/// Combines HOMA-IR (insulin resistance) with CGM time-in-range (`tir_percent`,
/// 0–100): low TIR worsens risk. A healthy baseline (HOMA-IR ≈ 1, TIR ≈ 70)
/// scores near 0; clinically concerning values approach 100.
pub fn insulin_resistance_risk_score(homa_ir: f64, tir_percent: f64) -> Result<f64> {
    if !(0.0..=100.0).contains(&tir_percent) {
        return Err(PathosError::InvalidInput(
            "tir_percent must be in [0, 100]".to_string(),
        ));
    }
    if homa_ir < 0.0 {
        return Err(PathosError::InvalidInput(
            "homa_ir must be non-negative".to_string(),
        ));
    }
    // HOMA-IR contribution saturates: 0 at IR<=1, 1 at IR>=4 (severe).
    let ir_component = ((homa_ir - 1.0) / 3.0).clamp(0.0, 1.0);
    // TIR contribution: 0 at TIR>=70, 1 at TIR<=40.
    let tir_component = ((70.0 - tir_percent) / 30.0).clamp(0.0, 1.0);
    let score = 100.0 * (0.6 * ir_component + 0.4 * tir_component);
    Ok(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_homa_ir_known_value() {
        // fasting glucose 90 mg/dL, insulin 9 µIU/mL -> 90*9/405 = 2.0
        assert!((homa_ir(90.0, 9.0).unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_homa_ir_rejects_nonpositive() {
        assert!(homa_ir(0.0, 9.0).is_err());
        assert!(homa_ir(90.0, 0.0).is_err());
    }

    #[test]
    fn test_metabolic_syndrome_three_criteria_met() {
        let input = MetabolicSyndromeInput {
            waist_cm: 105.0, // male -> waist criterion met
            is_female: false,
            triglycerides_mgdl: 160.0,
            hdl_mgdl: 35.0,
            systolic_bp: 125.0,
            diastolic_bp: 80.0,
            fasting_glucose_mgdl: 95.0,
            on_bp_meds: false,
            on_glucose_meds: false,
        };
        let r = metabolic_syndrome_assessment(&input).unwrap();
        assert!(r.is_metabolic_syndrome);
        assert_eq!(r.criteria_met, 3);
    }

    #[test]
    fn test_metabolic_syndrome_not_met() {
        let input = MetabolicSyndromeInput {
            waist_cm: 90.0,
            is_female: false,
            triglycerides_mgdl: 100.0,
            hdl_mgdl: 50.0,
            systolic_bp: 118.0,
            diastolic_bp: 75.0,
            fasting_glucose_mgdl: 90.0,
            on_bp_meds: false,
            on_glucose_meds: false,
        };
        let r = metabolic_syndrome_assessment(&input).unwrap();
        assert!(!r.is_metabolic_syndrome);
    }

    #[test]
    fn test_insulin_resistance_risk_score_bounds() {
        // Healthy: IR ~1, TIR 70 -> ~0
        let low = insulin_resistance_risk_score(1.0, 70.0).unwrap();
        assert!(low < 1.0);
        // Severe: IR 4, TIR 40 -> ~100
        let high = insulin_resistance_risk_score(4.0, 40.0).unwrap();
        assert!((high - 100.0).abs() < 1e-6);
        assert!(insulin_resistance_risk_score(1.0, 120.0).is_err());
    }
}
