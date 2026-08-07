//! Biomarker discovery / validation statistics (Phase 4, `tpt-soma-clinica`).

use crate::stats::{mean, pearson_r, sample_variance, welch_t_test};
use crate::{ClinicaError, Result};

/// Result of a biomarker association test.
#[derive(Debug, Clone)]
pub struct BiomarkerResult {
    pub statistic: f64,
    pub p_value: f64,
    pub effect_size: f64,
}

fn group_to_f64(g: &[bool]) -> Vec<f64> {
    g.iter().map(|b| if *b { 1.0 } else { 0.0 }).collect()
}

/// Point-biserial correlation between a continuous biomarker and a binary group
/// (case/control). Equivalent to Pearson correlation against the 0/1 encoding.
pub fn point_biserial_correlation(score: &[f64], group: &[bool]) -> Result<f64> {
    if score.len() != group.len() || score.len() < 2 {
        return Err(ClinicaError::Stats(
            "score and group must have equal length >= 2".to_string(),
        ));
    }
    pearson_r(score, &group_to_f64(group))
}

/// Associate a continuous biomarker with a binary outcome (case/control).
/// Returns a Welch t-statistic, two-tailed p-value, and Cohen's-d-style effect
/// size (mean difference / pooled SD).
pub fn associate_biomarker(values: &[f64], case: &[bool]) -> Result<BiomarkerResult> {
    if values.len() != case.len() || values.len() < 2 {
        return Err(ClinicaError::Stats(
            "values and case must have equal length >= 2".to_string(),
        ));
    }
    let cases: Vec<f64> = values
        .iter()
        .zip(case)
        .filter(|(_, g)| **g)
        .map(|(v, _)| *v)
        .collect();
    let controls: Vec<f64> = values
        .iter()
        .zip(case)
        .filter(|(_, g)| !**g)
        .map(|(v, _)| *v)
        .collect();
    if cases.len() < 2 || controls.len() < 2 {
        return Err(ClinicaError::Stats(
            "both groups need at least 2 points".to_string(),
        ));
    }
    let (t, _df, p) = welch_t_test(&cases, &controls)?;
    let nc = cases.len() as f64;
    let nk = controls.len() as f64;
    let pooled_sd = ((sample_variance(&cases) * (nc - 1.0)
        + sample_variance(&controls) * (nk - 1.0))
        / (nc + nk - 2.0))
        .sqrt();
    let effect = if pooled_sd > 0.0 {
        (mean(&cases) - mean(&controls)) / pooled_sd
    } else {
        0.0
    };
    Ok(BiomarkerResult {
        statistic: t,
        p_value: p,
        effect_size: effect,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_associate_biomarker_strong_separation() {
        // case group clearly higher than control
        let values = [1.0, 2.0, 3.0, 4.0, 10.0, 11.0, 12.0, 13.0];
        let case = [false, false, false, false, true, true, true, true];
        let r = associate_biomarker(&values, &case).unwrap();
        assert!(r.p_value < 1e-3, "p = {}", r.p_value);
        assert!(r.effect_size > 1.0, "d = {}", r.effect_size);
    }

    #[test]
    fn test_associate_biomarker_no_separation() {
        let values = [1.0, 2.0, 3.0, 4.0, 1.5, 2.5, 3.5, 4.5];
        let case = [false, false, false, false, true, true, true, true];
        let r = associate_biomarker(&values, &case).unwrap();
        assert!(r.p_value > 0.3, "p = {}", r.p_value);
    }

    #[test]
    fn test_point_biserial_sign_matches() {
        let score = [1.0, 2.0, 3.0, 10.0, 11.0, 12.0];
        let group = [false, false, false, true, true, true];
        let r = point_biserial_correlation(&score, &group).unwrap();
        assert!(r > 0.9, "r = {r}");
    }
}
