//! Organ function test calculators

use crate::{OrganonError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Ejection fraction calculation (cardiac)
pub fn ejection_fraction(end_diastolic_volume: f64, end_systolic_volume: f64) -> Result<f64> {
    if end_diastolic_volume <= 0.0 {
        return Err(OrganonError::InvalidInput(
            "End diastolic volume must be positive".to_string(),
        ));
    }
    if end_systolic_volume < 0.0 {
        return Err(OrganonError::InvalidInput(
            "End systolic volume cannot be negative".to_string(),
        ));
    }
    if end_systolic_volume > end_diastolic_volume {
        return Err(OrganonError::InvalidInput(
            "End systolic volume cannot exceed end diastolic volume".to_string(),
        ));
    }

    let ef = ((end_diastolic_volume - end_systolic_volume) / end_diastolic_volume) * 100.0;
    Ok(ef)
}

/// GFR calculation using CKD-EPI 2021 equation (renal)
pub fn gfr_ckd_epi_2021(
    creatinine: f64,
    age: u32,
    is_female: bool,
    is_black: bool,
) -> Result<f64> {
    if creatinine <= 0.0 {
        return Err(OrganonError::InvalidInput(
            "Creatinine must be positive".to_string(),
        ));
    }
    if age == 0 {
        return Err(OrganonError::InvalidInput("Age must be positive".to_string()));
    }

    let k = if is_female { 0.7 } else { 0.9 };
    let alpha = if is_female { -0.241 } else { -0.302 };
    let sex_factor = if is_female { 1.012 } else { 1.0 };
    let race_factor = if is_black { 1.159 } else { 1.0 };

    let scr_k = creatinine / k;
    let min_term = scr_k.min(1.0).powf(alpha);
    let max_term = scr_k.max(1.0).powf(-1.2);

    let gfr = 142.0 * min_term * max_term * (0.9938_f64.powi(age as i32)) * sex_factor * race_factor;
    Ok(gfr.max(0.0))
}

/// Pulmonary function indices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PulmonaryFunction {
    pub fev1: f64,        // Forced expiratory volume in 1 second (L)
    pub fvc: f64,         // Forced vital capacity (L)
    pub fev1_fvc_ratio: f64,
    pub pef: Option<f64>, // Peak expiratory flow (L/s)
}

impl PulmonaryFunction {
    pub fn new(fev1: f64, fvc: f64, pef: Option<f64>) -> Result<Self> {
        if fev1 <= 0.0 || fvc <= 0.0 {
            return Err(OrganonError::InvalidInput(
                "FEV1 and FVC must be positive".to_string(),
            ));
        }
        if fev1 > fvc {
            return Err(OrganonError::InvalidInput(
                "FEV1 cannot exceed FVC".to_string(),
            ));
        }

        Ok(Self {
            fev1,
            fvc,
            fev1_fvc_ratio: fev1 / fvc,
            pef,
        })
    }

    pub fn interpret(&self) -> PulmonaryInterpretation {
        if self.fev1_fvc_ratio < 0.7 {
            PulmonaryInterpretation::Obstructive
        } else if self.fvc < 3.0 {
            // Simplified - would use reference values in practice
            PulmonaryInterpretation::Restrictive
        } else {
            PulmonaryInterpretation::Normal
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PulmonaryInterpretation {
    Normal,
    Obstructive,
    Restrictive,
    Mixed,
}

/// Liver enzyme panel interpretation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiverPanel {
    pub alt: f64,  // U/L
    pub ast: f64,  // U/L
    pub alp: f64,  // U/L
    pub ggt: f64,  // U/L
    pub bili: f64, // mg/dL
}

impl LiverPanel {
    pub fn new(alt: f64, ast: f64, alp: f64, ggt: f64, bili: f64) -> Result<Self> {
        if alt < 0.0 || ast < 0.0 || alp < 0.0 || ggt < 0.0 || bili < 0.0 {
            return Err(OrganonError::InvalidInput(
                "All liver enzyme values must be non-negative".to_string(),
            ));
        }
        Ok(Self {
            alt,
            ast,
            alp,
            ggt,
            bili,
        })
    }

    pub fn ast_alt_ratio(&self) -> f64 {
        if self.alt > 0.0 {
            self.ast / self.alt
        } else {
            f64::INFINITY
        }
    }

    pub fn interpret(&self) -> LiverInterpretation {
        let ratio = self.ast_alt_ratio();
        let alt_uln = 40.0; // Upper limit of normal (simplified)
        let ast_uln = 40.0;

        if self.alt > 5.0 * alt_uln || self.ast > 5.0 * ast_uln {
            LiverInterpretation::AcuteHepatocellular
        } else if self.alp > 2.0 * 120.0 && self.ggt > 2.0 * 50.0 {
            LiverInterpretation::Cholestatic
        } else if ratio > 2.0 {
            LiverInterpretation::AlcoholicLiverDisease
        } else if ratio < 1.0 && (self.alt > alt_uln || self.ast > ast_uln) {
            LiverInterpretation::ViralHepatitis
        } else {
            LiverInterpretation::Normal
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LiverInterpretation {
    Normal,
    AcuteHepatocellular,
    Cholestatic,
    AlcoholicLiverDisease,
    ViralHepatitis,
    Mixed,
}

/// Clinical reference ranges (versioned config, not hardcoded)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceRanges {
    pub version: String,
    pub ranges: HashMap<String, (f64, f64)>, // (lower, upper)
}

impl ReferenceRanges {
    pub fn new(version: String) -> Self {
        let mut ranges = HashMap::new();
        // Default reference ranges (simplified)
        ranges.insert("creatinine".to_string(), (0.6, 1.3));
        ranges.insert("egfr".to_string(), (90.0, 200.0));
        ranges.insert("alt".to_string(), (7.0, 56.0));
        ranges.insert("ast".to_string(), (10.0, 40.0));
        ranges.insert("alp".to_string(), (44.0, 147.0));
        ranges.insert("ggt".to_string(), (9.0, 48.0));
        ranges.insert("bili_total".to_string(), (0.1, 1.2));
        ranges.insert("fev1".to_string(), (3.0, 5.0));
        ranges.insert("fvc".to_string(), (4.0, 6.0));
        ranges.insert("ef".to_string(), (55.0, 70.0));

        Self { version, ranges }
    }

    pub fn check(&self, test: &str, value: f64) -> ReferenceStatus {
        if let Some((low, high)) = self.ranges.get(test) {
            if value < *low {
                ReferenceStatus::Low
            } else if value > *high {
                ReferenceStatus::High
            } else {
                ReferenceStatus::Normal
            }
        } else {
            ReferenceStatus::Unknown
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReferenceStatus {
    Low,
    Normal,
    High,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ejection_fraction_normal() {
        let ef = ejection_fraction(120.0, 50.0).unwrap();
        assert!((ef - 58.33).abs() < 0.1);
    }

    #[test]
    fn test_ejection_fraction_invalid() {
        assert!(ejection_fraction(0.0, 50.0).is_err());
        assert!(ejection_fraction(100.0, 120.0).is_err());
    }

    #[test]
    fn test_gfr_ckd_epi() {
        let gfr = gfr_ckd_epi_2021(1.0, 40, false, false).unwrap();
        assert!(gfr > 60.0); // Normal GFR for 40yo male
    }

    #[test]
    fn test_pulmonary_function() {
        let pf = PulmonaryFunction::new(3.5, 4.5, Some(8.0)).unwrap();
        assert!((pf.fev1_fvc_ratio - 0.777).abs() < 0.01);
        assert_eq!(pf.interpret(), PulmonaryInterpretation::Normal);
    }

    #[test]
    fn test_liver_panel() {
        let panel = LiverPanel::new(30.0, 25.0, 80.0, 20.0, 0.8).unwrap();
        assert_eq!(panel.interpret(), LiverInterpretation::Normal);
    }

    #[test]
    fn test_reference_ranges() {
        let refs = ReferenceRanges::new("1.0".to_string());
        assert_eq!(refs.check("creatinine", 1.0), ReferenceStatus::Normal);
        assert_eq!(refs.check("creatinine", 2.0), ReferenceStatus::High);
        assert_eq!(refs.check("creatinine", 0.5), ReferenceStatus::Low);
    }
}