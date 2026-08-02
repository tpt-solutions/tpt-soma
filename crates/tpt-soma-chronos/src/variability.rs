//! Glycemic variability calculations: TIR, TBR, TAR, CV, MAGE

use crate::{ChronosError, Result};
use chrono::{DateTime, Duration, Utc};

/// Glycemic variability metrics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GlycemicVariability {
    pub subject_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub readings_count: usize,

    // Time in Range metrics
    pub tir: TimeInRangeMetrics,
    pub tar: TimeAboveRangeMetrics,
    pub tbr: TimeBelowRangeMetrics,

    // Variability metrics
    pub cv: f64,           // Coefficient of variation (%)
    pub sd: f64,           // Standard deviation (mg/dL)
    pub mean_glucose: f64, // Mean glucose (mg/dL)
    pub mage: f64,         // Mean Amplitude of Glycemic Excursions
    pub modd: f64,         // Mean of Daily Differences
    pub conga: f64,        // Continuous Overall Net Glycemic Action
    pub lability_index: f64,
    pub gmi: f64,          // Glucose Management Indicator (estimated HbA1c)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimeInRangeMetrics {
    pub very_low_pct: f64,   // < 54 mg/dL
    pub low_pct: f64,        // 54-69 mg/dL
    pub in_range_pct: f64,   // 70-180 mg/dL
    pub high_pct: f64,       // 181-250 mg/dL
    pub very_high_pct: f64,  // > 250 mg/dL
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimeAboveRangeMetrics {
    pub level_1_pct: f64, // 181-250 mg/dL
    pub level_2_pct: f64, // > 250 mg/dL
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimeBelowRangeMetrics {
    pub level_1_pct: f64, // 54-69 mg/dL
    pub level_2_pct: f64, // < 54 mg/dL
}

/// Calculate glycemic variability from CGM readings
pub fn calculate_glycemic_variability(
    subject_id: &str,
    readings: &[f64],
    timestamps: &[DateTime<Utc>],
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<GlycemicVariability> {
    if readings.len() != timestamps.len() {
        return Err(ChronosError::Variability(
            "Readings and timestamps length mismatch".to_string(),
        ));
    }
    if readings.is_empty() {
        return Err(ChronosError::Variability("No readings provided".to_string()));
    }

    // Filter readings within period
    let mut filtered_readings = Vec::new();
    for (i, ts) in timestamps.iter().enumerate() {
        if *ts >= period_start && *ts <= period_end {
            filtered_readings.push(readings[i]);
        }
    }

    if filtered_readings.is_empty() {
        return Err(ChronosError::Variability(
            "No readings in specified period".to_string(),
        ));
    }

    let n = filtered_readings.len() as f64;
    let mean = filtered_readings.iter().sum::<f64>() / n;
    let variance = filtered_readings.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let sd = variance.sqrt();
    let cv = (sd / mean) * 100.0;

    // Time in Range (standard ranges)
    let very_low = filtered_readings.iter().filter(|&&x| x < 54.0).count() as f64;
    let low = filtered_readings.iter().filter(|&&x| x >= 54.0 && x < 70.0).count() as f64;
    let in_range = filtered_readings.iter().filter(|&&x| x >= 70.0 && x <= 180.0).count() as f64;
    let high = filtered_readings.iter().filter(|&&x| x > 180.0 && x <= 250.0).count() as f64;
    let very_high = filtered_readings.iter().filter(|&&x| x > 250.0).count() as f64;

    let tir = TimeInRangeMetrics {
        very_low_pct: (very_low / n) * 100.0,
        low_pct: (low / n) * 100.0,
        in_range_pct: (in_range / n) * 100.0,
        high_pct: (high / n) * 100.0,
        very_high_pct: (very_high / n) * 100.0,
    };

    let tar = TimeAboveRangeMetrics {
        level_1_pct: (high / n) * 100.0,
        level_2_pct: (very_high / n) * 100.0,
    };

    let tbr = TimeBelowRangeMetrics {
        level_1_pct: (low / n) * 100.0,
        level_2_pct: (very_low / n) * 100.0,
    };

    // MAGE - Mean Amplitude of Glycemic Excursions
    let mage = calculate_mage(&filtered_readings);

    // MODD - Mean of Daily Differences
    let modd = calculate_modd(&filtered_readings, timestamps, period_start, period_end);

    // CONGA - Continuous Overall Net Glycemic Action (1-hour)
    let conga = calculate_conga(&filtered_readings, 60); // 60 minutes

    // Lability Index
    let lability_index = calculate_lability_index(&filtered_readings);

    // GMI - Glucose Management Indicator
    let gmi = calculate_gmi(mean);

    Ok(GlycemicVariability {
        subject_id: subject_id.to_string(),
        period_start,
        period_end,
        readings_count: filtered_readings.len(),
        tir,
        tar,
        tbr,
        cv,
        sd,
        mean_glucose: mean,
        mage,
        modd,
        conga,
        lability_index,
        gmi,
    })
}

/// Calculate MAGE (Mean Amplitude of Glycemic Excursions)
/// Based on the standard algorithm: identify peaks and nadirs, calculate amplitudes
fn calculate_mage(readings: &[f64]) -> f64 {
    if readings.len() < 3 {
        return 0.0;
    }

    let mut peaks = Vec::new();
    let mut nadirs = Vec::new();

    // Find local maxima (peaks) and minima (nadirs)
    for i in 1..readings.len() - 1 {
        if readings[i] > readings[i - 1] && readings[i] > readings[i + 1] {
            peaks.push((i, readings[i]));
        } else if readings[i] < readings[i - 1] && readings[i] < readings[i + 1] {
            nadirs.push((i, readings[i]));
        }
    }

    // Calculate excursions (peak-to-nadir or nadir-to-peak)
    let mut excursions = Vec::new();

    // Pair peaks with subsequent nadirs
    let mut peak_idx = 0;
    let mut nadir_idx = 0;

    while peak_idx < peaks.len() && nadir_idx < nadirs.len() {
        let (p_idx, p_val) = peaks[peak_idx];
        let (n_idx, n_val) = nadirs[nadir_idx];

        if n_idx > p_idx {
            // Nadir after peak - downward excursion
            let amplitude = (p_val - n_val).abs();
            if amplitude > 1.0 { // Only count significant excursions (>1 mg/dL)
                excursions.push(amplitude);
            }
            nadir_idx += 1;
        } else {
            peak_idx += 1;
        }
    }

    // Pair nadirs with subsequent peaks
    peak_idx = 0;
    nadir_idx = 0;

    while nadir_idx < nadirs.len() && peak_idx < peaks.len() {
        let (n_idx, n_val) = nadirs[nadir_idx];
        let (p_idx, p_val) = peaks[peak_idx];

        if p_idx > n_idx {
            // Peak after nadir - upward excursion
            let amplitude = (p_val - n_val).abs();
            if amplitude > 1.0 {
                excursions.push(amplitude);
            }
            peak_idx += 1;
        } else {
            nadir_idx += 1;
        }
    }

    if excursions.is_empty() {
        0.0
    } else {
        excursions.iter().sum::<f64>() / excursions.len() as f64
    }
}

/// Calculate MODD (Mean of Daily Differences)
fn calculate_modd(
    readings: &[f64],
    timestamps: &[DateTime<Utc>],
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> f64 {
    // Group readings by day
    let mut daily_readings: std::collections::HashMap<chrono::NaiveDate, Vec<f64>> =
        std::collections::HashMap::new();

    for (i, ts) in timestamps.iter().enumerate() {
        if *ts >= period_start && *ts <= period_end {
            let date = ts.date_naive();
            daily_readings.entry(date).or_default().push(readings[i]);
        }
    }

    if daily_readings.len() < 2 {
        return 0.0;
    }

    // Calculate daily means
    let mut daily_means: Vec<(chrono::NaiveDate, f64)> = daily_readings
        .into_iter()
        .map(|(date, vals)| (date, vals.iter().sum::<f64>() / vals.len() as f64))
        .collect();

    daily_means.sort_by_key(|(date, _)| *date);

    // Calculate differences between consecutive days
    let mut differences = Vec::new();
    for i in 1..daily_means.len() {
        let diff = (daily_means[i].1 - daily_means[i - 1].1).abs();
        differences.push(diff);
    }

    if differences.is_empty() {
        0.0
    } else {
        differences.iter().sum::<f64>() / differences.len() as f64
    }
}

/// Calculate CONGA (Continuous Overall Net Glycemic Action)
fn calculate_conga(readings: &[f64], window_minutes: usize) -> f64 {
    // Simplified: assume 5-minute intervals, so window = window_minutes / 5 readings
    let window_size = window_minutes / 5;
    if readings.len() <= window_size {
        return 0.0;
    }

    let mut differences = Vec::new();
    for i in window_size..readings.len() {
        let diff = (readings[i] - readings[i - window_size]).abs();
        differences.push(diff);
    }

    if differences.is_empty() {
        0.0
    } else {
        let mean = differences.iter().sum::<f64>() / differences.len() as f64;
        let variance = differences.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / differences.len() as f64;
        variance.sqrt()
    }
}

/// Calculate Lability Index
fn calculate_lability_index(readings: &[f64]) -> f64 {
    if readings.len() < 2 {
        return 0.0;
    }

    let mut sum_squared_diff = 0.0;
    for i in 1..readings.len() {
        let diff = readings[i] - readings[i - 1];
        sum_squared_diff += diff * diff;
    }

    (sum_squared_diff / (readings.len() - 1) as f64).sqrt()
}

/// Calculate GMI (Glucose Management Indicator) - estimated HbA1c
/// Formula: GMI = 3.31 + 0.02392 * mean_glucose (mg/dL)
fn calculate_gmi(mean_glucose: f64) -> f64 {
    3.31 + 0.02392 * mean_glucose
}

/// Calculate circadian/ultradian rhythm analysis
pub fn analyze_rhythms(
    readings: &[f64],
    timestamps: &[DateTime<Utc>],
) -> Result<RhythmAnalysis> {
    if readings.len() != timestamps.len() || readings.len() < 288 { // At least 24h at 5-min intervals
        return Err(ChronosError::Variability(
            "Insufficient data for rhythm analysis (need >24h)".to_string(),
        ));
    }

    // Detrend the data
    let mean = readings.iter().sum::<f64>() / readings.len() as f64;
    let detrended: Vec<f64> = readings.iter().map(|x| x - mean).collect();

    // Find dominant periods using autocorrelation
    let periods = find_dominant_periods(&detrended);

    // Classify rhythms
    let mut circadian_strength = 0.0f64;
    let mut ultradian_strength = 0.0f64;

    for (period_hours, strength) in &periods {
        if (22.0..=26.0).contains(period_hours) {
            circadian_strength = *strength;
        } else if (1.0..=6.0).contains(period_hours) {
            ultradian_strength = ultradian_strength.max(*strength);
        }
    }

    Ok(RhythmAnalysis {
        circadian_strength,
        ultradian_strength,
        dominant_periods: periods,
    })
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RhythmAnalysis {
    pub circadian_strength: f64,
    pub ultradian_strength: f64,
    pub dominant_periods: Vec<(f64, f64)>, // (period_hours, strength)
}

fn find_dominant_periods(signal: &[f64]) -> Vec<(f64, f64)> {
    // Simplified autocorrelation-based period detection
    let n = signal.len();
    let max_lag = n / 2;
    let mut autocorr = Vec::new();

    for lag in 1..max_lag {
        let mut sum = 0.0;
        let mut count = 0;
        for i in 0..n - lag {
            sum += signal[i] * signal[i + lag];
            count += 1;
        }
        if count > 0 {
            autocorr.push((lag, sum / count as f64));
        }
    }

    // Find peaks in autocorrelation
    let mut peaks = Vec::new();
    for i in 1..autocorr.len() - 1 {
        if autocorr[i].1 > autocorr[i - 1].1 && autocorr[i].1 > autocorr[i + 1].1 {
            let period_hours = (autocorr[i].0 * 5) as f64 / 60.0; // 5-min intervals to hours
            peaks.push((period_hours, autocorr[i].1));
        }
    }

    // Sort by strength
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    peaks.truncate(5); // Top 5

    peaks
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_calculate_glycemic_variability() {
        let readings: Vec<f64> = (0..288).map(|i| 100.0 + 20.0 * (i as f64 * 0.1).sin()).collect();
        let timestamps: Vec<DateTime<Utc>> = (0..288)
            .map(|i| Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap() + Duration::minutes(i * 5))
            .collect();

        let period_start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let period_end = Utc.with_ymd_and_hms(2024, 1, 1, 23, 55, 0).unwrap();

        let gv = calculate_glycemic_variability("patient-1", &readings, &timestamps, period_start, period_end).unwrap();

        assert!(gv.mean_glucose > 90.0 && gv.mean_glucose < 110.0);
        assert!(gv.cv > 0.0 && gv.cv < 50.0);
        assert!(gv.tir.in_range_pct > 0.0);
        assert!(gv.gmi > 4.0 && gv.gmi < 10.0);
    }

    #[test]
    fn test_mage() {
        // Create oscillating pattern
        let readings: Vec<f64> = vec![100.0, 120.0, 100.0, 80.0, 100.0, 130.0, 100.0, 70.0, 100.0];
        let mage = calculate_mage(&readings);
        assert!(mage > 0.0);
    }

    #[test]
    fn test_gmi() {
        let gmi = calculate_gmi(150.0);
        assert!((gmi - 6.9).abs() < 0.1); // ~6.9% for mean 150 mg/dL
    }

    #[test]
    fn test_lability_index() {
        let readings = vec![100.0, 110.0, 105.0, 115.0, 100.0];
        let li = calculate_lability_index(&readings);
        assert!(li > 0.0);
    }
}