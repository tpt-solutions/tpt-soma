//! Statistics toolkit for clinica (Phase 4).

use crate::{ClinicaError, Result};

/// Arithmetic mean.
pub fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

/// Sample variance (n-1 denominator). Returns 0 for < 2 points.
pub fn sample_variance(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let m = mean(xs);
    let ss: f64 = xs.iter().map(|x| (x - m).powi(2)).sum();
    ss / (xs.len() - 1) as f64
}

/// Error function via Abramowitz & Stegun 7.1.26.
fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let ax = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * ax);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-ax * ax).exp();
    sign * y
}

/// Standard normal CDF.
fn normal_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / 2.0_f64.sqrt()))
}

/// Welch's t-test for two samples with (possibly) unequal variances.
/// Returns `(t_statistic, degrees_of_freedom, two_tailed_p_value)`.
pub fn welch_t_test(a: &[f64], b: &[f64]) -> Result<(f64, f64, f64)> {
    if a.len() < 2 || b.len() < 2 {
        return Err(ClinicaError::Stats(
            "both samples need at least 2 points".to_string(),
        ));
    }
    let ma = mean(a);
    let mb = mean(b);
    let va = sample_variance(a);
    let vb = sample_variance(b);
    let na = a.len() as f64;
    let nb = b.len() as f64;
    let sa = va / na;
    let sb = vb / nb;
    let denom = (sa + sb).sqrt();
    if denom == 0.0 {
        return Err(ClinicaError::Stats("zero variance sum".to_string()));
    }
    let t = (ma - mb) / denom;
    let df_num = (sa + sb).powi(2);
    let df_den = sa.powi(2) / (na - 1.0) + sb.powi(2) / (nb - 1.0);
    let df = if df_den == 0.0 {
        na + nb - 2.0
    } else {
        df_num / df_den
    };
    let p = 2.0 * (1.0 - normal_cdf(t.abs()));
    Ok((t, df, p.clamp(0.0, 1.0)))
}

/// Pearson correlation coefficient.
pub fn pearson_r(x: &[f64], y: &[f64]) -> Result<f64> {
    if x.len() != y.len() || x.len() < 2 {
        return Err(ClinicaError::Stats(
            "x and y must have equal length >= 2".to_string(),
        ));
    }
    let mx = mean(x);
    let my = mean(y);
    let mut num = 0.0;
    let mut dx2 = 0.0;
    let mut dy2 = 0.0;
    for i in 0..x.len() {
        let dx = x[i] - mx;
        let dy = y[i] - my;
        num += dx * dy;
        dx2 += dx * dx;
        dy2 += dy * dy;
    }
    let denom = (dx2 * dy2).sqrt();
    if denom == 0.0 {
        return Err(ClinicaError::Stats("zero variance in input".to_string()));
    }
    Ok(num / denom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_welch_detects_difference() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [10.0, 11.0, 12.0, 13.0, 14.0];
        let (t, _df, p) = welch_t_test(&a, &b).unwrap();
        assert!(t < 0.0);
        assert!(p < 1e-3, "p = {p}");
    }

    #[test]
    fn test_welch_no_difference_high_p() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [1.5, 2.5, 3.5, 4.5, 5.5];
        let (_t, _df, p) = welch_t_test(&a, &b).unwrap();
        assert!(p > 0.5, "p = {p}");
    }

    #[test]
    fn test_pearson_perfect_positive() {
        let x = [1.0, 2.0, 3.0, 4.0];
        let y = [2.0, 4.0, 6.0, 8.0];
        let r = pearson_r(&x, &y).unwrap();
        assert!((r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_pearson_negative() {
        let x = [1.0, 2.0, 3.0, 4.0];
        let y = [4.0, 3.0, 2.0, 1.0];
        let r = pearson_r(&x, &y).unwrap();
        assert!((r + 1.0).abs() < 1e-9);
    }
}
