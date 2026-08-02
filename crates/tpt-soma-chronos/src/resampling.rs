//! 5-minute interval resampling and gap-filling logic for continuous sensor data

use crate::{ChronosError, Result};
use chrono::{DateTime, Duration, Utc};
use std::collections::VecDeque;

/// Resampling configuration
#[derive(Debug, Clone)]
pub struct ResampleConfig {
    pub interval_minutes: i64,
    pub max_gap_minutes: i64,
    pub interpolation_method: InterpolationMethod,
}

impl Default for ResampleConfig {
    fn default() -> Self {
        Self {
            interval_minutes: 5,
            max_gap_minutes: 60,
            interpolation_method: InterpolationMethod::Linear,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterpolationMethod {
    Linear,
    Nearest,
    Cubic,
    Previous,
    Next,
}

/// Resample time series to regular intervals with gap filling
pub fn resample_time_series(
    timestamps: &[DateTime<Utc>],
    values: &[f64],
    config: &ResampleConfig,
) -> Result<(Vec<DateTime<Utc>>, Vec<f64>)> {
    if timestamps.len() != values.len() {
        return Err(ChronosError::Resampling(
            "Timestamps and values length mismatch".to_string(),
        ));
    }
    if timestamps.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let interval = Duration::minutes(config.interval_minutes);
    let max_gap = Duration::minutes(config.max_gap_minutes);

    // Find time range
    let start = *timestamps.first().unwrap();
    let end = *timestamps.last().unwrap();

    // Generate regular time grid
    let mut regular_times = Vec::new();
    let mut current = start;
    while current <= end {
        regular_times.push(current);
        current = current + interval;
    }

    // Interpolate values at regular times
    let mut regular_values = Vec::with_capacity(regular_times.len());

    for &target_time in &regular_times {
        let value = interpolate_at_time(timestamps, values, target_time, config)?;
        regular_values.push(value);
    }

    // Fill gaps larger than max_gap with NaN
    let mut final_times = Vec::new();
    let mut final_values = Vec::new();

    for i in 0..regular_times.len() {
        if i == 0 {
            final_times.push(regular_times[i]);
            final_values.push(regular_values[i]);
            continue;
        }

        let gap = regular_times[i] - regular_times[i - 1];
        if gap > max_gap {
            // Gap too large - insert NaN
            final_values.push(f64::NAN);
        } else {
            final_values.push(regular_values[i]);
        }
        final_times.push(regular_times[i]);
    }

    Ok((final_times, final_values))
}

/// Interpolate value at a specific time
fn interpolate_at_time(
    timestamps: &[DateTime<Utc>],
    values: &[f64],
    target: DateTime<Utc>,
    config: &ResampleConfig,
) -> Result<f64> {
    // Find surrounding points
    let idx = timestamps
        .binary_search(&target)
        .unwrap_or_else(|i| i);

    match config.interpolation_method {
        InterpolationMethod::Nearest => {
            if idx == 0 {
                Ok(values[0])
            } else if idx >= timestamps.len() {
                Ok(values[values.len() - 1])
            } else {
                let before = timestamps[idx - 1];
                let after = timestamps[idx];
                if (target - before) < (after - target) {
                    Ok(values[idx - 1])
                } else {
                    Ok(values[idx])
                }
            }
        }
        InterpolationMethod::Previous => {
            if idx == 0 {
                Ok(values[0])
            } else {
                Ok(values[idx - 1])
            }
        }
        InterpolationMethod::Next => {
            if idx >= timestamps.len() {
                Ok(values[values.len() - 1])
            } else {
                Ok(values[idx])
            }
        }
        InterpolationMethod::Linear => {
            if idx == 0 {
                Ok(values[0])
            } else if idx >= timestamps.len() {
                Ok(values[values.len() - 1])
            } else {
                let t0 = timestamps[idx - 1];
                let t1 = timestamps[idx];
                let v0 = values[idx - 1];
                let v1 = values[idx];

                let total_secs = (t1 - t0).num_seconds() as f64;
                let target_secs = (target - t0).num_seconds() as f64;

                if total_secs == 0.0 {
                    Ok(v0)
                } else {
                    let ratio = target_secs / total_secs;
                    Ok(v0 + ratio * (v1 - v0))
                }
            }
        }
        InterpolationMethod::Cubic => {
            // Simplified cubic - fallback to linear for now
            interpolate_at_time(timestamps, values, target, &ResampleConfig {
                interpolation_method: InterpolationMethod::Linear,
                ..*config
            })
        }
    }
}

/// Gap filling using forward/backward fill with optional interpolation
pub fn fill_gaps(
    timestamps: &mut Vec<DateTime<Utc>>,
    values: &mut Vec<f64>,
    max_gap_minutes: i64,
    method: GapFillMethod,
) -> Result<()> {
    if timestamps.len() != values.len() {
        return Err(ChronosError::GapFilling(
            "Timestamps and values length mismatch".to_string(),
        ));
    }
    if timestamps.len() < 2 {
        return Ok(());
    }

    let max_gap = Duration::minutes(max_gap_minutes);
    let mut i = 1;

    while i < timestamps.len() {
        let gap = timestamps[i] - timestamps[i - 1];
        if gap > max_gap && gap <= max_gap * 10 {
            // Fill this gap
            let num_points = (gap.num_minutes() / 5) as usize; // 5-min intervals
            let t0 = timestamps[i - 1];
            let t1 = timestamps[i];
            let v0 = values[i - 1];
            let v1 = values[i];

            let mut new_times = Vec::new();
            let mut new_values = Vec::new();

            for j in 1..=num_points {
                let ratio = j as f64 / (num_points + 1) as f64;
                let new_time = t0 + Duration::minutes((gap.num_minutes() as f64 * ratio) as i64);
                let new_value = match method {
                    GapFillMethod::Linear => v0 + ratio * (v1 - v0),
                    GapFillMethod::ForwardFill => v0,
                    GapFillMethod::BackwardFill => v1,
                    GapFillMethod::Mean => (v0 + v1) / 2.0,
                };
                new_times.push(new_time);
                new_values.push(new_value);
            }

            // Insert at position i
            for (t, v) in new_times.into_iter().zip(new_values).rev() {
                timestamps.insert(i, t);
                values.insert(i, v);
            }

            i += num_points + 1;
        } else {
            i += 1;
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GapFillMethod {
    Linear,
    ForwardFill,
    BackwardFill,
    Mean,
}

/// Rolling window statistics for time series
pub fn rolling_statistics(
    values: &[f64],
    window_size: usize,
    stat: RollingStat,
) -> Vec<f64> {
    if window_size == 0 || values.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::with_capacity(values.len());
    let mut window = VecDeque::new();

    for &v in values {
        window.push_back(v);
        if window.len() > window_size {
            window.pop_front();
        }

        let result = match stat {
            RollingStat::Mean => window.iter().sum::<f64>() / window.len() as f64,
            RollingStat::Std => {
                let mean = window.iter().sum::<f64>() / window.len() as f64;
                let variance = window.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / window.len() as f64;
                variance.sqrt()
            }
            RollingStat::Min => window.iter().fold(f64::INFINITY, |a, &b| a.min(b)),
            RollingStat::Max => window.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b)),
            RollingStat::Median => {
                let mut sorted: Vec<f64> = window.iter().copied().collect();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let mid = sorted.len() / 2;
                if sorted.len() % 2 == 0 {
                    (sorted[mid - 1] + sorted[mid]) / 2.0
                } else {
                    sorted[mid]
                }
            }
        };
        results.push(result);
    }

    results
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RollingStat {
    Mean,
    Std,
    Min,
    Max,
    Median,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_resample_linear() {
        let timestamps = vec![
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 10, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 20, 0).unwrap(),
        ];
        let values = vec![100.0, 120.0, 140.0];

        let config = ResampleConfig {
            interval_minutes: 5,
            max_gap_minutes: 60,
            interpolation_method: InterpolationMethod::Linear,
        };

        let (times, vals) = resample_time_series(&timestamps, &values, &config).unwrap();
        assert_eq!(times.len(), 5); // 0, 5, 10, 15, 20 minutes
        assert!((vals[0] - 100.0).abs() < 0.01);
        assert!((vals[2] - 120.0).abs() < 0.01);
        assert!((vals[4] - 140.0).abs() < 0.01);
    }

    #[test]
    fn test_fill_gaps() {
        let mut timestamps = vec![
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 20, 0).unwrap(), // 20 min gap
        ];
        let mut values = vec![100.0, 140.0];

        fill_gaps(&mut timestamps, &mut values, 10, GapFillMethod::Linear).unwrap();

        assert_eq!(timestamps.len(), 5); // Original 2 + 3 filled
        assert!((values[1] - 110.0).abs() < 0.01); // 100 + 10
        assert!((values[2] - 120.0).abs() < 0.01); // 100 + 20
        assert!((values[3] - 130.0).abs() < 0.01); // 100 + 30
    }

    #[test]
    fn test_rolling_mean() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = rolling_statistics(&values, 3, RollingStat::Mean);
        assert_eq!(result.len(), 5);
        assert!((result[0] - 1.0).abs() < 0.01);
        assert!((result[2] - 2.0).abs() < 0.01); // (1+2+3)/3
        assert!((result[4] - 4.0).abs() < 0.01); // (3+4+5)/3
    }
}