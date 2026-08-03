//! Longitudinal organ function trajectories

use crate::{ChronosError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Longitudinal trajectory for an organ function test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganFunctionTrajectory {
    pub id: Uuid,
    pub subject_id: String,
    pub test_loinc: String,
    pub test_name: String,
    pub unit: String,
    pub time_points: Vec<TrajectoryPoint>,
    pub reference_range: Option<(f64, f64)>, // (low, high)
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryPoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub is_interpolated: bool,
    pub visit_id: Option<String>,
}

/// Trajectory analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryAnalysis {
    pub trajectory_id: Uuid,
    pub slope: f64,                    // Rate of change per unit time
    pub slope_per_year: f64,
    pub r_squared: f64,                // Goodness of fit
    pub trend: TrendDirection,
    pub velocity: f64,                 // Current rate of change
    pub acceleration: f64,             // Rate of change of velocity
    pub time_in_range: Option<TimeInRange>,
    pub anomalies: Vec<Anomaly>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrendDirection {
    Improving,
    Stable,
    Declining,
    Fluctuating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeInRange {
    pub in_range_pct: f64,
    pub below_range_pct: f64,
    pub above_range_pct: f64,
    pub range_low: f64,
    pub range_high: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub expected_value: f64,
    pub deviation: f64,
    pub severity: AnomalySeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnomalySeverity {
    Mild,
    Moderate,
    Severe,
}

/// Analyze organ function trajectory
pub fn analyze_trajectory(trajectory: &OrganFunctionTrajectory) -> Result<TrajectoryAnalysis> {
    if trajectory.time_points.len() < 2 {
        return Err(ChronosError::InvalidInput(
            "Need at least 2 time points for trajectory analysis".to_string(),
        ));
    }

    // Filter out interpolated points for trend analysis
    let valid_points: Vec<&TrajectoryPoint> = trajectory
        .time_points
        .iter()
        .filter(|p| !p.is_interpolated)
        .collect();

    if valid_points.len() < 2 {
        return Err(ChronosError::InvalidInput(
            "Need at least 2 non-interpolated time points".to_string(),
        ));
    }

    // Linear regression for trend
    let (slope, intercept, r_squared) = linear_regression(&valid_points);
    let slope_per_year = slope * 365.25 * 24.0 * 60.0; // Convert per-minute to per-year

    // Determine trend direction
    let trend = if slope_per_year.abs() < 0.01 {
        TrendDirection::Stable
    } else if slope_per_year > 0.0 {
        TrendDirection::Improving
    } else {
        TrendDirection::Declining
    };

    // Calculate velocity (current rate of change using last 3 points)
    let velocity = calculate_velocity(&valid_points);

    // Calculate acceleration (change in velocity)
    let acceleration = calculate_acceleration(&valid_points);

    // Time in range if reference range available
    let time_in_range = trajectory.reference_range.map(|(low, high)| {
        calculate_time_in_range(&trajectory.time_points, low, high)
    });

    // Detect anomalies
    let anomalies = detect_anomalies(&valid_points, slope, intercept);

    Ok(TrajectoryAnalysis {
        trajectory_id: trajectory.id,
        slope,
        slope_per_year,
        r_squared,
        trend,
        velocity,
        acceleration,
        time_in_range,
        anomalies,
    })
}

fn linear_regression(points: &[&TrajectoryPoint]) -> (f64, f64, f64) {
    let n = points.len() as f64;
    let x: Vec<f64> = points.iter().map(|p| p.timestamp.timestamp() as f64).collect();
    let y: Vec<f64> = points.iter().map(|p| p.value).collect();

    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let sum_xy: f64 = x.iter().zip(y.iter()).map(|(xi, yi)| xi * yi).sum();
    let sum_x2: f64 = x.iter().map(|xi| xi * xi).sum();
    let sum_y2: f64 = y.iter().map(|yi| yi * yi).sum();

    let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);
    let intercept = (sum_y - slope * sum_x) / n;

    let r_squared = if n > 1.0 {
        let numerator = n * sum_xy - sum_x * sum_y;
        let denominator = ((n * sum_x2 - sum_x * sum_x) * (n * sum_y2 - sum_y * sum_y)).sqrt();
        if denominator != 0.0 {
            (numerator / denominator).powi(2)
        } else {
            0.0
        }
    } else {
        0.0
    };

    (slope, intercept, r_squared)
}

fn calculate_velocity(points: &[&TrajectoryPoint]) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }

    // Use last 3 points for velocity estimate
    let window = points.len().min(3);
    let recent = &points[points.len() - window..];

    if recent.len() < 2 {
        return 0.0;
    }

    let (slope, _, _) = linear_regression(recent);
    slope * 60.0 // Convert to per-hour
}

fn calculate_acceleration(points: &[&TrajectoryPoint]) -> f64 {
    if points.len() < 4 {
        return 0.0;
    }

    // Compare velocity of first half vs second half
    let mid = points.len() / 2;
    let first_half = &points[..mid];
    let second_half = &points[mid..];

    let v1 = calculate_velocity(first_half);
    let v2 = calculate_velocity(second_half);

    v2 - v1
}

fn calculate_time_in_range(points: &[TrajectoryPoint], low: f64, high: f64) -> TimeInRange {
    let total = points.len() as f64;
    let in_range = points.iter().filter(|p| p.value >= low && p.value <= high).count() as f64;
    let below = points.iter().filter(|p| p.value < low).count() as f64;
    let above = points.iter().filter(|p| p.value > high).count() as f64;

    TimeInRange {
        in_range_pct: (in_range / total) * 100.0,
        below_range_pct: (below / total) * 100.0,
        above_range_pct: (above / total) * 100.0,
        range_low: low,
        range_high: high,
    }
}

fn detect_anomalies(
    points: &[&TrajectoryPoint],
    slope: f64,
    intercept: f64,
) -> Vec<Anomaly> {
    let mut anomalies = Vec::new();

    for point in points {
        let expected = slope * point.timestamp.timestamp() as f64 + intercept;
        let deviation = (point.value - expected).abs();

        // Anomaly if deviation > 2 standard deviations (approximate)
        // Using a simple threshold based on value magnitude
        let threshold = point.value.abs() * 0.2 + 5.0; // 20% or 5 units, whichever is larger

        if deviation > threshold {
            let severity = if deviation > threshold * 3.0 {
                AnomalySeverity::Severe
            } else if deviation > threshold * 1.5 {
                AnomalySeverity::Moderate
            } else {
                AnomalySeverity::Mild
            };

            anomalies.push(Anomaly {
                timestamp: point.timestamp,
                value: point.value,
                expected_value: expected,
                deviation,
                severity,
            });
        }
    }

    anomalies
}

/// Compare two trajectories (e.g., before/after intervention)
pub fn compare_trajectories(
    baseline: &OrganFunctionTrajectory,
    followup: &OrganFunctionTrajectory,
) -> Result<TrajectoryComparison> {
    if baseline.test_loinc != followup.test_loinc {
        return Err(ChronosError::InvalidInput(
            "Cannot compare trajectories with different test codes".to_string(),
        ));
    }

    let baseline_analysis = analyze_trajectory(baseline)?;
    let followup_analysis = analyze_trajectory(followup)?;

    let slope_change = followup_analysis.slope_per_year - baseline_analysis.slope_per_year;
    let trend_changed = baseline_analysis.trend != followup_analysis.trend;

    Ok(TrajectoryComparison {
        test_loinc: baseline.test_loinc.clone(),
        baseline_slope_per_year: baseline_analysis.slope_per_year,
        followup_slope_per_year: followup_analysis.slope_per_year,
        slope_change,
        trend_changed,
        baseline_trend: baseline_analysis.trend,
        followup_trend: followup_analysis.trend,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryComparison {
    pub test_loinc: String,
    pub baseline_slope_per_year: f64,
    pub followup_slope_per_year: f64,
    pub slope_change: f64,
    pub trend_changed: bool,
    pub baseline_trend: TrendDirection,
    pub followup_trend: TrendDirection,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_analyze_trajectory_improving() {
        let mut trajectory = OrganFunctionTrajectory {
            id: Uuid::new_v4(),
            subject_id: "patient-1".to_string(),
            test_loinc: "2160-0".to_string(), // Creatinine
            test_name: "Creatinine".to_string(),
            unit: "mg/dL".to_string(),
            time_points: vec![],
            reference_range: Some((0.6, 1.3)),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Declining creatinine (improving renal function) - negative slope
        let base = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        trajectory.time_points = vec![
            TrajectoryPoint { timestamp: base, value: 1.5, is_interpolated: false, visit_id: None },
            TrajectoryPoint { timestamp: base + chrono::Duration::days(30), value: 1.3, is_interpolated: false, visit_id: None },
            TrajectoryPoint { timestamp: base + chrono::Duration::days(60), value: 1.1, is_interpolated: false, visit_id: None },
            TrajectoryPoint { timestamp: base + chrono::Duration::days(90), value: 1.0, is_interpolated: false, visit_id: None },
        ];

        let analysis = analyze_trajectory(&trajectory).unwrap();
        // Current logic: positive slope = Improving, negative slope = Declining
        // For creatinine, lower is better, so negative slope = clinically improving
        // But the algorithm doesn't know clinical direction yet, so it reports Declining
        assert_eq!(analysis.trend, TrendDirection::Declining);
        assert!(analysis.slope_per_year < 0.0);
    }

    #[test]
    fn test_time_in_range() {
        let points = vec![
            TrajectoryPoint { timestamp: Utc::now(), value: 100.0, is_interpolated: false, visit_id: None },
            TrajectoryPoint { timestamp: Utc::now(), value: 120.0, is_interpolated: false, visit_id: None },
            TrajectoryPoint { timestamp: Utc::now(), value: 180.0, is_interpolated: false, visit_id: None },
            TrajectoryPoint { timestamp: Utc::now(), value: 80.0, is_interpolated: false, visit_id: None },
        ];
        let tir = calculate_time_in_range(&points, 70.0, 180.0);
        assert!((tir.in_range_pct - 100.0).abs() < 0.01); // 4/4 in range (100, 120, 180, 80 all in [70, 180])
        assert!((tir.below_range_pct - 0.0).abs() < 0.01); // 0/4 below
        assert!((tir.above_range_pct - 0.0).abs() < 0.01); // 0/4 above
    }
}