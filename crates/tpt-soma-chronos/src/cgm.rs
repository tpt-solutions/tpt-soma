//! CGM (Continuous Glucose Monitoring) data ingestion and parsing

use crate::{ChronosError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// CGM reading from a sensor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgmReading {
    pub id: Uuid,
    pub subject_id: String,
    pub timestamp: DateTime<Utc>,
    pub glucose_mgdl: f64,
    pub source: CgmSource,
    pub sensor_id: Option<String>,
    pub is_calibrated: bool,
    pub trend_arrow: Option<TrendArrow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CgmSource {
    DexcomG6,
    DexcomG7,
    Libre1,
    Libre2,
    Libre3,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrendArrow {
    RisingRapidly,    // > 3 mg/dL/min
    Rising,           // 2-3 mg/dL/min
    RisingSlightly,   // 1-2 mg/dL/min
    Stable,           // -1 to 1 mg/dL/min
    FallingSlightly,  // -1 to -2 mg/dL/min
    Falling,          // -2 to -3 mg/dL/min
    FallingRapidly,   // < -3 mg/dL/min
    Unknown,
}

/// Dexcom binary stream parser (simplified)
pub mod dexcom {
    use super::*;

    /// Parse Dexcom binary data (placeholder - real implementation would parse actual binary format)
    pub fn parse_dexcom_stream(_data: &[u8]) -> Result<Vec<CgmReading>> {
        Err(ChronosError::Resampling(
            "Dexcom binary parsing not yet implemented".to_string(),
        ))
    }

    /// Parse Dexcom CSV export
    pub fn parse_dexcom_csv(content: &str) -> Result<Vec<CgmReading>> {
        let mut rdr = csv::Reader::from_reader(content.as_bytes());
        let mut readings = Vec::new();

        for result in rdr.deserialize() {
            let record: DexcomCsvRecord = result.map_err(|e| {
                ChronosError::Resampling(format!("CSV parse error: {}", e))
            })?;

            let timestamp = DateTime::parse_from_rfc3339(&record.timestamp)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| ChronosError::Resampling(format!("Invalid timestamp: {}", e)))?;

            readings.push(CgmReading {
                id: Uuid::new_v4(),
                subject_id: record.subject_id,
                timestamp,
                glucose_mgdl: record.glucose_mgdl,
                source: CgmSource::DexcomG6,
                sensor_id: Some(record.sensor_id),
                is_calibrated: record.is_calibrated,
                trend_arrow: record.trend_arrow.map(|s| parse_trend_arrow(&s)),
            });
        }

        Ok(readings)
    }

    #[derive(Debug, Deserialize)]
    struct DexcomCsvRecord {
        subject_id: String,
        timestamp: String,
        glucose_mgdl: f64,
        sensor_id: String,
        is_calibrated: bool,
        trend_arrow: Option<String>,
    }

    fn parse_trend_arrow(s: &str) -> TrendArrow {
        match s.to_lowercase().as_str() {
            "rising_rapidly" | "↑↑" => TrendArrow::RisingRapidly,
            "rising" | "↑" => TrendArrow::Rising,
            "rising_slightly" => TrendArrow::RisingSlightly,
            "stable" | "→" => TrendArrow::Stable,
            "falling_slightly" => TrendArrow::FallingSlightly,
            "falling" | "↓" => TrendArrow::Falling,
            "falling_rapidly" | "↓↓" => TrendArrow::FallingRapidly,
            _ => TrendArrow::Unknown,
        }
    }
}

/// Libre binary stream parser (simplified)
pub mod libre {
    use super::*;

    /// Parse Libre binary data (placeholder)
    pub fn parse_libre_stream(_data: &[u8]) -> Result<Vec<CgmReading>> {
        Err(ChronosError::Resampling(
            "Libre binary parsing not yet implemented".to_string(),
        ))
    }

    /// Parse Libre CSV export
    pub fn parse_libre_csv(content: &str) -> Result<Vec<CgmReading>> {
        let mut rdr = csv::Reader::from_reader(content.as_bytes());
        let mut readings = Vec::new();

        for result in rdr.deserialize() {
            let record: LibreCsvRecord = result.map_err(|e| {
                ChronosError::Resampling(format!("CSV parse error: {}", e))
            })?;

            let timestamp = DateTime::parse_from_rfc3339(&record.timestamp)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| ChronosError::Resampling(format!("Invalid timestamp: {}", e)))?;

            readings.push(CgmReading {
                id: Uuid::new_v4(),
                subject_id: record.subject_id,
                timestamp,
                glucose_mgdl: record.glucose_mgdl,
                source: CgmSource::Libre2,
                sensor_id: Some(record.sensor_id),
                is_calibrated: false, // Libre is factory calibrated
                trend_arrow: record.trend_arrow.map(|s| parse_trend_arrow(&s)),
            });
        }

        Ok(readings)
    }

    #[derive(Debug, Deserialize)]
    struct LibreCsvRecord {
        subject_id: String,
        timestamp: String,
        glucose_mgdl: f64,
        sensor_id: String,
        trend_arrow: Option<String>,
    }

    fn parse_trend_arrow(s: &str) -> TrendArrow {
        match s.to_lowercase().as_str() {
            "rising_rapidly" => TrendArrow::RisingRapidly,
            "rising" => TrendArrow::Rising,
            "rising_slightly" => TrendArrow::RisingSlightly,
            "stable" => TrendArrow::Stable,
            "falling_slightly" => TrendArrow::FallingSlightly,
            "falling" => TrendArrow::Falling,
            "falling_rapidly" => TrendArrow::FallingRapidly,
            _ => TrendArrow::Unknown,
        }
    }
}

/// Validate CGM readings
pub fn validate_cgm_readings(readings: &[CgmReading]) -> Result<()> {
    if readings.is_empty() {
        return Err(ChronosError::InvalidInput("No readings provided".to_string()));
    }

    for reading in readings {
        if reading.glucose_mgdl < 20.0 || reading.glucose_mgdl > 600.0 {
            return Err(ChronosError::InvalidInput(format!(
                "Glucose value out of physiological range: {} mg/dL",
                reading.glucose_mgdl
            )));
        }
    }

    // Check for duplicate timestamps
    let mut seen = HashMap::new();
    for reading in readings {
        let key = (reading.subject_id.clone(), reading.timestamp);
        if seen.contains_key(&key) {
            return Err(ChronosError::InvalidInput(format!(
                "Duplicate reading for subject {} at {}",
                reading.subject_id, reading.timestamp
            )));
        }
        seen.insert(key, true);
    }

    Ok(())
}

/// Sort readings by timestamp
pub fn sort_readings(readings: &mut [CgmReading]) {
    readings.sort_by_key(|r| r.timestamp);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cgm_readings() {
        let readings = vec![
            CgmReading {
                id: Uuid::new_v4(),
                subject_id: "patient-1".to_string(),
                timestamp: Utc::now(),
                glucose_mgdl: 100.0,
                source: CgmSource::DexcomG6,
                sensor_id: None,
                is_calibrated: true,
                trend_arrow: None,
            },
        ];
        assert!(validate_cgm_readings(&readings).is_ok());
    }

    #[test]
    fn test_validate_cgm_readings_out_of_range() {
        let readings = vec![
            CgmReading {
                id: Uuid::new_v4(),
                subject_id: "patient-1".to_string(),
                timestamp: Utc::now(),
                glucose_mgdl: 1000.0, // Out of range
                source: CgmSource::DexcomG6,
                sensor_id: None,
                is_calibrated: true,
                trend_arrow: None,
            },
        ];
        assert!(validate_cgm_readings(&readings).is_err());
    }
}