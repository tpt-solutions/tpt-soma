//! Organ imaging ingestion: MRI/CT/ultrasound/PET metadata + blob storage

use crate::{OrganonError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// DICOM metadata for organ imaging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DicomMetadata {
    pub study_instance_uid: String,
    pub series_instance_uid: String,
    pub sop_instance_uid: String,
    pub patient_id: String,
    pub patient_name: Option<String>,
    pub study_date: Option<String>,
    pub study_time: Option<String>,
    pub modality: ImagingModality,
    pub body_part_examined: Option<String>,
    pub rows: u32,
    pub columns: u32,
    pub bits_allocated: u16,
    pub bits_stored: u16,
    pub pixel_spacing: Option<(f64, f64)>,
    pub slice_thickness: Option<f64>,
    pub manufacturer: Option<String>,
    pub manufacturer_model: Option<String>,
    pub institution_name: Option<String>,
    pub series_description: Option<String>,
    pub protocol_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ImagingModality {
    CT,
    MR,
    US,   // Ultrasound
    PT,   // PET
    XA,   // X-ray angiography
    RF,   // Radiofluoroscopy
    MG,   // Mammography
    DX,   // Digital radiography
    CR,   // Computed radiography
    Other(String),
}

impl std::str::FromStr for ImagingModality {
    type Err = OrganonError;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s.to_uppercase().as_str() {
            "CT" => ImagingModality::CT,
            "MR" => ImagingModality::MR,
            "US" => ImagingModality::US,
            "PT" => ImagingModality::PT,
            "XA" => ImagingModality::XA,
            "RF" => ImagingModality::RF,
            "MG" => ImagingModality::MG,
            "DX" => ImagingModality::DX,
            "CR" => ImagingModality::CR,
            other => ImagingModality::Other(other.to_string()),
        })
    }
}

/// Organ imaging record for storage (metadata in Keystone, pixel data in MinIO)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganImagingRecord {
    pub id: Uuid,
    pub subject_id: String,
    pub dicom_metadata: DicomMetadata,
    pub minio_bucket: String,
    pub minio_object_key: String,
    pub checksum_sha256: String,
    pub file_size_bytes: u64,
    pub ingested_at: DateTime<Utc>,
    pub organ_system: Option<String>, // UBERON code
    pub laterality: Option<Laterality>,
    pub view_position: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Laterality {
    Left,
    Right,
    Bilateral,
    Unilateral,
    Midline,
    Unknown,
}

/// Parse DICOM metadata from a file (placeholder - would use dicom crate in practice)
pub fn parse_dicom_metadata(_file_path: &str) -> Result<DicomMetadata> {
    // In practice, this would use the `dicom` crate to parse actual DICOM files
    // For now, return a placeholder
    Err(OrganonError::Imaging(
        "DICOM parsing not yet implemented - requires dicom crate".to_string(),
    ))
}

/// Validate DICOM metadata for organ imaging
pub fn validate_dicom_metadata(meta: &DicomMetadata) -> Result<()> {
    if meta.study_instance_uid.is_empty() {
        return Err(OrganonError::Imaging(
            "Missing study instance UID".to_string(),
        ));
    }
    if meta.series_instance_uid.is_empty() {
        return Err(OrganonError::Imaging(
            "Missing series instance UID".to_string(),
        ));
    }
    if meta.patient_id.is_empty() {
        return Err(OrganonError::Imaging("Missing patient ID".to_string()));
    }
    if meta.rows == 0 || meta.columns == 0 {
        return Err(OrganonError::Imaging(
            "Invalid image dimensions".to_string(),
        ));
    }
    Ok(())
}

/// Generate MinIO object key for organ imaging
pub fn generate_imaging_object_key(
    subject_id: &str,
    study_uid: &str,
    series_uid: &str,
    instance_uid: &str,
) -> String {
    format!(
        "organ-imaging/{}/{}/{}/{}.dcm",
        subject_id, study_uid, series_uid, instance_uid
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imaging_modality_from_str() {
        assert_eq!("CT".parse::<ImagingModality>().unwrap(), ImagingModality::CT);
        assert_eq!("MR".parse::<ImagingModality>().unwrap(), ImagingModality::MR);
        assert_eq!("US".parse::<ImagingModality>().unwrap(), ImagingModality::US);
        assert_eq!("PT".parse::<ImagingModality>().unwrap(), ImagingModality::PT);
        assert!(matches!(
            "XX".parse::<ImagingModality>().unwrap(),
            ImagingModality::Other(_)
        ));
    }

    #[test]
    fn test_generate_imaging_object_key() {
        let key = generate_imaging_object_key("patient-1", "study-1", "series-1", "instance-1");
        assert_eq!(
            key,
            "organ-imaging/patient-1/study-1/series-1/instance-1.dcm"
        );
    }
}