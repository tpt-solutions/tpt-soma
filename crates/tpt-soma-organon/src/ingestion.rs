//! FHIR R5 subset parser and CSV/manual upload ingestion for organ function panels

use crate::{OrganonError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// FHIR R5 Patient resource (subset)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirPatient {
    pub id: String,
    pub identifier: Vec<FhirIdentifier>,
    pub name: Vec<FhirHumanName>,
    pub gender: Option<String>,
    pub birth_date: Option<String>,
    pub deceased: Option<FhirDeceased>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirIdentifier {
    pub system: Option<String>,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirHumanName {
    pub family: Option<String>,
    pub given: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FhirDeceased {
    Boolean(bool),
    DateTime(String),
}

/// FHIR R5 Observation resource (subset for organ function panels)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirObservation {
    pub id: String,
    pub status: String,
    pub category: Vec<FhirCodeableConcept>,
    pub code: FhirCodeableConcept,
    pub subject: FhirReference,
    pub effective: Option<FhirEffective>,
    pub value: Option<FhirValue>,
    pub interpretation: Vec<FhirCodeableConcept>,
    pub reference_range: Vec<FhirReferenceRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirCodeableConcept {
    pub coding: Vec<FhirCoding>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirCoding {
    pub system: Option<String>,
    pub code: String,
    pub display: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirReference {
    pub reference: String,
    pub display: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FhirEffective {
    DateTime(String),
    Period(FhirPeriod),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirPeriod {
    pub start: String,
    pub end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FhirValue {
    Quantity(FhirQuantity),
    CodeableConcept(FhirCodeableConcept),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirQuantity {
    pub value: f64,
    pub unit: Option<String>,
    pub system: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirReferenceRange {
    pub low: Option<FhirQuantity>,
    pub high: Option<FhirQuantity>,
    pub text: Option<String>,
}

/// LOINC codes for organ function panels
pub mod loinc {
    // Cardiac
    pub const EJECTION_FRACTION: &str = "18043-0";
    pub const TROPONIN_I: &str = "10839-9";
    pub const TROPONIN_T: &str = "6598-7";
    pub const BNP: &str = "33762-8";
    pub const NT_PRO_BNP: &str = "33763-6";

    // Renal
    pub const CREATININE: &str = "2160-0";
    pub const EGFR: &str = "62238-1";
    pub const BUN: &str = "3094-0";
    pub const CYSTATIN_C: &str = "30214-8";

    // Hepatic
    pub const ALT: &str = "1742-6";
    pub const AST: &str = "1920-8";
    pub const ALP: &str = "6768-6";
    pub const GGT: &str = "2324-2";
    pub const BILIRUBIN_TOTAL: &str = "1975-2";
    pub const BILIRUBIN_DIRECT: &str = "1968-7";
    pub const ALBUMIN: &str = "1751-7";

    // Pulmonary
    pub const FEV1: &str = "19914-1";
    pub const FVC: &str = "19915-8";
    pub const FEV1_FVC_RATIO: &str = "19916-6";
    pub const PEF: &str = "19917-4";

    // Endocrine
    pub const HBA1C: &str = "4548-4";
    pub const GLUCOSE_FASTING: &str = "1558-6";
    pub const GLUCOSE_RANDOM: &str = "2345-7";
    pub const INSULIN: &str = "20446-8";
    pub const TSH: &str = "3016-3";
    pub const T4_FREE: &str = "3026-2";
    pub const T3_FREE: &str = "3029-6";

    // Lipids
    pub const CHOLESTEROL_TOTAL: &str = "2093-3";
    pub const HDL: &str = "2085-9";
    pub const LDL: &str = "18262-6";
    pub const TRIGLYCERIDES: &str = "2571-8";
}

/// Parse FHIR Observation into normalized clinical observation
pub fn parse_fhir_observation(obs: &FhirObservation) -> Result<ClinicalObservation> {
    let code = obs
        .code
        .coding
        .first()
        .ok_or_else(|| OrganonError::Ingestion("Observation missing code".to_string()))?;

    let loinc_code = code.code.clone();
    let value = match &obs.value {
        Some(FhirValue::Quantity(q)) => q.value,
        Some(FhirValue::String(s)) => s.parse().map_err(|_| {
            OrganonError::Ingestion(format!("Cannot parse string value as number: {}", s))
        })?,
        _ => {
            return Err(OrganonError::Ingestion(
                "Observation missing numeric value".to_string(),
            ));
        }
    };

    let unit = match &obs.value {
        Some(FhirValue::Quantity(q)) => q.unit.clone().unwrap_or_default(),
        _ => String::new(),
    };

    let effective_time = match &obs.effective {
        Some(FhirEffective::DateTime(dt)) => DateTime::parse_from_rfc3339(dt)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| OrganonError::Ingestion(format!("Invalid datetime: {}", e)))?,
        Some(FhirEffective::Period(p)) => DateTime::parse_from_rfc3339(&p.start)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| OrganonError::Ingestion(format!("Invalid period start: {}", e)))?,
        None => Utc::now(),
    };

    let subject_id = obs
        .subject
        .reference
        .strip_prefix("Patient/")
        .unwrap_or(&obs.subject.reference)
        .to_string();

    Ok(ClinicalObservation {
        id: Uuid::new_v4(),
        subject_id,
        loinc_code,
        value,
        unit,
        effective_time,
        status: obs.status.clone(),
        interpretation: obs
            .interpretation
            .iter()
            .flat_map(|c| c.coding.iter().map(|c| c.code.clone()))
            .collect(),
    })
}

/// Normalized clinical observation for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClinicalObservation {
    pub id: Uuid,
    pub subject_id: String,
    pub loinc_code: String,
    pub value: f64,
    pub unit: String,
    pub effective_time: DateTime<Utc>,
    pub status: String,
    pub interpretation: Vec<String>,
}

/// CSV ingestion for organ function panels (manual upload path)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganFunctionCsvRecord {
    pub patient_id: String,
    pub test_date: String,
    pub test_name: String,
    pub value: f64,
    pub unit: String,
    pub reference_range_low: Option<f64>,
    pub reference_range_high: Option<f64>,
}

pub fn parse_organ_function_csv(content: &str) -> Result<Vec<OrganFunctionCsvRecord>> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let mut records = Vec::new();

    for result in rdr.deserialize() {
        let record: OrganFunctionCsvRecord =
            result.map_err(|e| OrganonError::Ingestion(format!("CSV parse error: {}", e)))?;
        records.push(record);
    }

    Ok(records)
}

/// Convert CSV record to clinical observation
pub fn csv_to_clinical_observation(record: OrganFunctionCsvRecord) -> Result<ClinicalObservation> {
    let loinc_code = map_test_name_to_loinc(&record.test_name).ok_or_else(|| {
        OrganonError::Ingestion(format!("Unknown test name: {}", record.test_name))
    })?;

    let effective_time = DateTime::parse_from_rfc3339(&record.test_date)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| OrganonError::Ingestion(format!("Invalid date: {}", e)))?;

    Ok(ClinicalObservation {
        id: Uuid::new_v4(),
        subject_id: record.patient_id,
        loinc_code: loinc_code.to_string(),
        value: record.value,
        unit: record.unit,
        effective_time,
        status: "final".to_string(),
        interpretation: Vec::new(),
    })
}

fn map_test_name_to_loinc(test_name: &str) -> Option<&'static str> {
    let mapping: HashMap<&str, &str> = [
        ("ejection_fraction", loinc::EJECTION_FRACTION),
        ("troponin_i", loinc::TROPONIN_I),
        ("troponin_t", loinc::TROPONIN_T),
        ("bnp", loinc::BNP),
        ("nt_pro_bnp", loinc::NT_PRO_BNP),
        ("creatinine", loinc::CREATININE),
        ("egfr", loinc::EGFR),
        ("bun", loinc::BUN),
        ("cystatin_c", loinc::CYSTATIN_C),
        ("alt", loinc::ALT),
        ("ast", loinc::AST),
        ("alp", loinc::ALP),
        ("ggt", loinc::GGT),
        ("bilirubin_total", loinc::BILIRUBIN_TOTAL),
        ("bilirubin_direct", loinc::BILIRUBIN_DIRECT),
        ("albumin", loinc::ALBUMIN),
        ("fev1", loinc::FEV1),
        ("fvc", loinc::FVC),
        ("fev1_fvc_ratio", loinc::FEV1_FVC_RATIO),
        ("pef", loinc::PEF),
        ("hba1c", loinc::HBA1C),
        ("glucose_fasting", loinc::GLUCOSE_FASTING),
        ("glucose_random", loinc::GLUCOSE_RANDOM),
        ("insulin", loinc::INSULIN),
        ("tsh", loinc::TSH),
        ("t4_free", loinc::T4_FREE),
        ("t3_free", loinc::T3_FREE),
        ("cholesterol_total", loinc::CHOLESTEROL_TOTAL),
        ("hdl", loinc::HDL),
        ("ldl", loinc::LDL),
        ("triglycerides", loinc::TRIGLYCERIDES),
    ]
    .into_iter()
    .collect();

    mapping.get(test_name.to_lowercase().as_str()).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_test_name_to_loinc() {
        assert_eq!(
            map_test_name_to_loinc("creatinine"),
            Some(loinc::CREATININE)
        );
        assert_eq!(map_test_name_to_loinc("ALT"), Some(loinc::ALT));
        assert_eq!(map_test_name_to_loinc("unknown"), None);
    }

    #[test]
    fn test_parse_csv() {
        let csv_content = r#"patient_id,test_date,test_name,value,unit,reference_range_low,reference_range_high
patient-1,2024-01-15T10:00:00Z,creatinine,1.0,mg/dL,0.6,1.3
patient-1,2024-01-15T10:00:00Z,alt,30,U/L,7,56"#;

        let records = parse_organ_function_csv(csv_content).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].test_name, "creatinine");
        assert_eq!(records[0].value, 1.0);
    }
}
