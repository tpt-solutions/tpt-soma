use tpt_soma_organon::ingestion::{
    csv_to_clinical_observation, loinc, parse_fhir_observation, parse_organ_function_csv,
    FhirObservation,
};

/// Golden-file test: a FHIR R5 Observation resource (serum creatinine) parses into
/// a normalized clinical observation with the correct LOINC code, value, and subject.
#[test]
fn test_fhir_observation_creatinine_golden_file() {
    let fhir_json = r#"{
        "id": "obs-creatinine-1",
        "status": "final",
        "category": [],
        "code": {
            "coding": [
                { "system": "http://loinc.org", "code": "2160-0", "display": "Creatinine" }
            ],
            "text": "Creatinine"
        },
        "subject": { "reference": "Patient/patient-1" },
        "effective": "2024-03-15T09:30:00Z",
        "value": { "value": 1.1, "unit": "mg/dL", "system": "http://unitsofmeasure.org", "code": "mg/dL" },
        "interpretation": [],
        "reference_range": []
    }"#;

    let observation: FhirObservation = serde_json::from_str(fhir_json).unwrap();
    let parsed = parse_fhir_observation(&observation).unwrap();

    assert_eq!(parsed.subject_id, "patient-1");
    assert_eq!(parsed.loinc_code, loinc::CREATININE);
    assert_eq!(parsed.value, 1.1);
    assert_eq!(parsed.unit, "mg/dL");
    assert_eq!(parsed.status, "final");
}

/// Golden-file test: a FHIR Observation with a Period-typed effective time and no
/// interpretation still parses (Period.start is used as the effective timestamp).
#[test]
fn test_fhir_observation_period_effective_golden_file() {
    let fhir_json = r#"{
        "id": "obs-hba1c-1",
        "status": "final",
        "category": [],
        "code": {
            "coding": [
                { "system": "http://loinc.org", "code": "4548-4", "display": "Hemoglobin A1c" }
            ],
            "text": "HbA1c"
        },
        "subject": { "reference": "Patient/patient-2" },
        "effective": { "start": "2024-06-01T00:00:00Z", "end": null },
        "value": { "value": 6.8, "unit": "%", "system": null, "code": null },
        "interpretation": [
            { "coding": [{ "system": null, "code": "H", "display": null }], "text": null }
        ],
        "reference_range": []
    }"#;

    let observation: FhirObservation = serde_json::from_str(fhir_json).unwrap();
    let parsed = parse_fhir_observation(&observation).unwrap();

    assert_eq!(parsed.subject_id, "patient-2");
    assert_eq!(parsed.loinc_code, loinc::HBA1C);
    assert_eq!(parsed.value, 6.8);
    assert_eq!(parsed.interpretation, vec!["H".to_string()]);
}

/// Golden-file test: the CSV manual-upload path for organ function panels.
#[test]
fn test_organ_function_csv_golden_file() {
    let csv_content = "patient_id,test_date,test_name,value,unit,reference_range_low,reference_range_high\n\
patient-1,2024-01-15T10:00:00Z,creatinine,1.0,mg/dL,0.6,1.3\n\
patient-1,2024-01-15T10:00:00Z,egfr,95.0,mL/min/1.73m2,90,200\n";

    let records = parse_organ_function_csv(csv_content).unwrap();
    assert_eq!(records.len(), 2);

    let observation = csv_to_clinical_observation(records[0].clone()).unwrap();
    assert_eq!(observation.subject_id, "patient-1");
    assert_eq!(observation.loinc_code, loinc::CREATININE);
    assert_eq!(observation.value, 1.0);
}
