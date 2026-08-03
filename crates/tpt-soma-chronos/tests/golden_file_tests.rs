use tpt_soma_chronos::cgm::{dexcom, libre, sort_readings, validate_cgm_readings, CgmSource, TrendArrow};

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
