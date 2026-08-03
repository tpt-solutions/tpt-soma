use chrono::{DateTime, Utc};
use tpt_soma_chronos::cgm::{
    CgmSource, TrendArrow, dexcom, libre, sort_readings, validate_cgm_readings,
};

/// Golden-file test: Dexcom CSV export parses into normalized CGM readings with
/// trend arrows decoded and calibration flag preserved.
#[test]
fn test_dexcom_csv_golden_file() {
    let csv_content = "subject_id,timestamp,glucose_mgdl,sensor_id,is_calibrated,trend_arrow\n\
patient-1,2024-01-01T00:00:00Z,110.0,sensor-a,true,stable\n\
patient-1,2024-01-01T00:05:00Z,125.0,sensor-a,true,rising\n\
patient-1,2024-01-01T00:10:00Z,95.0,sensor-a,false,falling_rapidly\n";

    let mut readings = dexcom::parse_dexcom_csv(csv_content).unwrap();
    assert_eq!(readings.len(), 3);
    assert_eq!(readings[0].source, CgmSource::DexcomG6);
    assert_eq!(readings[0].glucose_mgdl, 110.0);
    assert_eq!(readings[1].trend_arrow, Some(TrendArrow::Rising));
    assert_eq!(readings[2].trend_arrow, Some(TrendArrow::FallingRapidly));
    assert!(!readings[2].is_calibrated);

    validate_cgm_readings(&readings).unwrap();
    sort_readings(&mut readings);
    assert!(readings[0].timestamp <= readings[1].timestamp);
}

/// Golden-file test: Libre CSV export parses into normalized CGM readings (Libre
/// sensors are factory-calibrated, so `is_calibrated` is always false).
#[test]
fn test_libre_csv_golden_file() {
    let csv_content = "subject_id,timestamp,glucose_mgdl,sensor_id,trend_arrow\n\
patient-2,2024-02-01T00:00:00Z,140.0,sensor-b,rising_slightly\n\
patient-2,2024-02-01T00:15:00Z,160.0,sensor-b,rising\n";

    let readings = libre::parse_libre_csv(csv_content).unwrap();
    assert_eq!(readings.len(), 2);
    assert_eq!(readings[0].source, CgmSource::Libre2);
    assert!(!readings[0].is_calibrated);
    assert_eq!(readings[0].trend_arrow, Some(TrendArrow::RisingSlightly));

    validate_cgm_readings(&readings).unwrap();
}

/// Golden-file test: physiologically implausible glucose values are rejected by
/// validation regardless of source.
#[test]
fn test_cgm_validation_rejects_out_of_range_golden_file() {
    let csv_content = "subject_id,timestamp,glucose_mgdl,sensor_id,is_calibrated,trend_arrow\n\
patient-3,2024-01-01T00:00:00Z,900.0,sensor-c,true,stable\n";

    let readings = dexcom::parse_dexcom_csv(csv_content).unwrap();
    assert!(validate_cgm_readings(&readings).is_err());
}

/// Test Dexcom binary stream parser
#[test]
fn test_dexcom_binary_stream() {
    // Create a binary record: subject_id (8 bytes) | timestamp (8 bytes, i64 ms) | glucose (4 bytes, f32) | sensor_id (4 bytes, u32)
    let mut data = Vec::new();

    // Record 1
    data.extend_from_slice(b"patient1"); // 8 bytes subject_id
    let ts1 = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    data.extend_from_slice(&ts1.timestamp_millis().to_le_bytes()); // 8 bytes timestamp
    data.extend_from_slice(&(110.0f32).to_le_bytes()); // 4 bytes glucose
    data.extend_from_slice(&12345u32.to_le_bytes()); // 4 bytes sensor_id

    // Record 2
    data.extend_from_slice(b"patient2"); // 8 bytes subject_id
    let ts2 = DateTime::parse_from_rfc3339("2024-01-01T00:05:00Z")
        .unwrap()
        .with_timezone(&Utc);
    data.extend_from_slice(&ts2.timestamp_millis().to_le_bytes()); // 8 bytes timestamp
    data.extend_from_slice(&(125.0f32).to_le_bytes()); // 4 bytes glucose
    data.extend_from_slice(&67890u32.to_le_bytes()); // 4 bytes sensor_id

    let readings = dexcom::parse_dexcom_stream(&data).unwrap();
    assert_eq!(readings.len(), 2);
    assert_eq!(readings[0].subject_id, "patient1");
    assert_eq!(readings[0].glucose_mgdl, 110.0);
    assert_eq!(readings[0].sensor_id, Some("12345".to_string()));
    assert_eq!(readings[1].subject_id, "patient2");
    assert_eq!(readings[1].glucose_mgdl, 125.0);
    assert_eq!(readings[1].sensor_id, Some("67890".to_string()));
    assert!(readings[0].is_calibrated);
    assert_eq!(readings[0].source, CgmSource::DexcomG6);
}

/// Test Libre binary stream parser
#[test]
fn test_libre_binary_stream() {
    // Create a binary record: subject_id (8 bytes) | timestamp (8 bytes, i64 ms) | glucose (4 bytes, f32) | sensor_id (4 bytes, u32)
    let mut data = Vec::new();

    // Record 1
    data.extend_from_slice(b"patient1"); // 8 bytes subject_id
    let ts1 = DateTime::parse_from_rfc3339("2024-02-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    data.extend_from_slice(&ts1.timestamp_millis().to_le_bytes()); // 8 bytes timestamp
    data.extend_from_slice(&(140.0f32).to_le_bytes()); // 4 bytes glucose
    data.extend_from_slice(&11111u32.to_le_bytes()); // 4 bytes sensor_id

    // Record 2
    data.extend_from_slice(b"patient2"); // 8 bytes subject_id
    let ts2 = DateTime::parse_from_rfc3339("2024-02-01T00:15:00Z")
        .unwrap()
        .with_timezone(&Utc);
    data.extend_from_slice(&ts2.timestamp_millis().to_le_bytes()); // 8 bytes timestamp
    data.extend_from_slice(&(160.0f32).to_le_bytes()); // 4 bytes glucose
    data.extend_from_slice(&22222u32.to_le_bytes()); // 4 bytes sensor_id

    let readings = libre::parse_libre_stream(&data).unwrap();
    assert_eq!(readings.len(), 2);
    assert_eq!(readings[0].subject_id, "patient1");
    assert_eq!(readings[0].glucose_mgdl, 140.0);
    assert_eq!(readings[0].sensor_id, Some("11111".to_string()));
    assert_eq!(readings[1].subject_id, "patient2");
    assert_eq!(readings[1].glucose_mgdl, 160.0);
    assert_eq!(readings[1].sensor_id, Some("22222".to_string()));
    assert!(!readings[0].is_calibrated); // Libre is factory calibrated
    assert_eq!(readings[0].source, CgmSource::Libre2);
}

/// Test binary stream parser rejects invalid length
#[test]
fn test_binary_stream_invalid_length() {
    let data = vec![0u8; 23]; // Not a multiple of 24
    assert!(dexcom::parse_dexcom_stream(&data).is_err());
    assert!(libre::parse_libre_stream(&data).is_err());
}
