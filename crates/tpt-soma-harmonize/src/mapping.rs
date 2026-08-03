use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Ontology source for mappings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OntologySource {
    /// dbSNP variant identifiers (rsIDs)
    DbSNP,
    /// HGNC gene symbols
    HGNC,
    /// LOINC codes for clinical observations
    LOINC,
    /// SNOMED CT clinical terms
    SNOMED,
    /// UBERON anatomy ontology
    UBERON,
    /// Custom/other source
    Other,
}

impl std::fmt::Display for OntologySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OntologySource::DbSNP => write!(f, "dbSNP"),
            OntologySource::HGNC => write!(f, "HGNC"),
            OntologySource::LOINC => write!(f, "LOINC"),
            OntologySource::SNOMED => write!(f, "SNOMED"),
            OntologySource::UBERON => write!(f, "UBERON"),
            OntologySource::Other => write!(f, "Other"),
        }
    }
}

impl std::str::FromStr for OntologySource {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "dbsnp" => Ok(OntologySource::DbSNP),
            "hgnc" => Ok(OntologySource::HGNC),
            "loinc" => Ok(OntologySource::LOINC),
            "snomed" | "snomedct" => Ok(OntologySource::SNOMED),
            "uberon" => Ok(OntologySource::UBERON),
            _ => Ok(OntologySource::Other),
        }
    }
}

/// A mapping entry with source ontology information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingEntry {
    pub source: OntologySource,
    pub target: String,
    pub confidence: f32, // 0.0 to 1.0
    pub notes: Option<String>,
}

impl MappingEntry {
    pub fn new(source: OntologySource, target: impl Into<String>) -> Self {
        Self {
            source,
            target: target.into(),
            confidence: 1.0,
            notes: None,
        }
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }
}

/// Multi-ontology mapping table
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MappingTable {
    /// Key format: "source:identifier" e.g., "dbsnp:1:100:A:T" or "hgnc:BRCA1"
    #[serde(flatten)]
    pub mappings: HashMap<String, MappingEntry>,
}

impl MappingTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a mapping with explicit ontology source
    pub fn insert(
        &mut self,
        source: OntologySource,
        identifier: impl Into<String>,
        entry: MappingEntry,
    ) {
        let key = format!("{}:{}", source, identifier.into());
        self.mappings.insert(key, entry);
    }

    /// Insert a simple mapping (defaults to confidence 1.0)
    pub fn insert_simple(
        &mut self,
        source: OntologySource,
        identifier: impl Into<String>,
        target: impl Into<String>,
    ) {
        self.insert(source, identifier, MappingEntry::new(source, target));
    }

    /// Resolve an identifier to its mapped value
    pub fn resolve(&self, source: OntologySource, identifier: &str) -> Option<&str> {
        let key = format!("{}:{}", source, identifier);
        self.mappings.get(&key).map(|e| e.target.as_str())
    }

    /// Resolve with full entry (including confidence and notes)
    pub fn resolve_entry(&self, source: OntologySource, identifier: &str) -> Option<&MappingEntry> {
        let key = format!("{}:{}", source, identifier);
        self.mappings.get(&key)
    }

    /// Get all mappings for a specific ontology source
    pub fn get_by_source(&self, source: OntologySource) -> Vec<(&String, &MappingEntry)> {
        let prefix = format!("{}:", source);
        self.mappings
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.mappings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &MappingEntry)> {
        self.mappings.iter()
    }

    /// Seed with common Phase 1/2 mappings
    pub fn seed_phase1_phase2() -> Self {
        let mut table = Self::new();

        // dbSNP variant mappings
        table.insert_simple(OntologySource::DbSNP, "1:100:A:T", "rs123");
        table.insert_simple(OntologySource::DbSNP, "1:200:C:G", "rs456");
        table.insert_simple(OntologySource::DbSNP, "17:43044295:G:A", "rs113488022"); // BRCA1
        table.insert_simple(OntologySource::DbSNP, "13:32319626:G:A", "rs80357906"); // BRCA2

        // HGNC gene mappings
        table.insert_simple(OntologySource::HGNC, "BRCA1", "HGNC:1100");
        table.insert_simple(OntologySource::HGNC, "BRCA2", "HGNC:1101");
        table.insert_simple(OntologySource::HGNC, "TP53", "HGNC:11998");
        table.insert_simple(OntologySource::HGNC, "EGFR", "HGNC:3236");

        // LOINC clinical observation mappings
        table.insert_simple(OntologySource::LOINC, "creatinine", "2160-0");
        table.insert_simple(OntologySource::LOINC, "egfr", "33914-3");
        table.insert_simple(OntologySource::LOINC, "hba1c", "4548-4");
        table.insert_simple(OntologySource::LOINC, "alt", "1742-6");
        table.insert_simple(OntologySource::LOINC, "ast", "1920-8");
        table.insert_simple(OntologySource::LOINC, "alp", "6768-6");
        table.insert_simple(OntologySource::LOINC, "ggt", "2324-2");
        table.insert_simple(OntologySource::LOINC, "bili_total", "1975-2");
        table.insert_simple(OntologySource::LOINC, "fev1", "20150-3");
        table.insert_simple(OntologySource::LOINC, "fvc", "20151-1");
        table.insert_simple(OntologySource::LOINC, "ef", "18043-0");

        // SNOMED CT clinical term mappings
        table.insert_simple(
            OntologySource::SNOMED,
            "diabetes_mellitus_type_2",
            "44054006",
        );
        table.insert_simple(OntologySource::SNOMED, "hypertension", "38341003");
        table.insert_simple(
            OntologySource::SNOMED,
            "chronic_kidney_disease",
            "709044004",
        );
        table.insert_simple(OntologySource::SNOMED, "heart_failure", "84114007");
        table.insert_simple(
            OntologySource::SNOMED,
            "acute_myocardial_infarction",
            "22298006",
        );
        table.insert_simple(OntologySource::SNOMED, "asthma", "195967001");
        table.insert_simple(OntologySource::SNOMED, "copd", "13645005");
        table.insert_simple(OntologySource::SNOMED, "atrial_fibrillation", "49436004");
        table.insert_simple(OntologySource::SNOMED, "stroke", "230690007");
        table.insert_simple(OntologySource::SNOMED, "obesity", "414916001");
        table.insert_simple(OntologySource::SNOMED, "dyslipidemia", "55822004");
        table.insert_simple(OntologySource::SNOMED, "hypothyroidism", "40930008");
        table.insert_simple(OntologySource::SNOMED, "hyperthyroidism", "363732003");
        table.insert_simple(OntologySource::SNOMED, "anemia", "271737000");
        table.insert_simple(OntologySource::SNOMED, "depression", "35489007");
        table.insert_simple(OntologySource::SNOMED, "anxiety_disorder", "48694002");
        table.insert_simple(
            OntologySource::SNOMED,
            "chronic_obstructive_pulmonary_disease",
            "13645005",
        );
        table.insert_simple(OntologySource::SNOMED, "chronic_liver_disease", "235856003");
        table.insert_simple(OntologySource::SNOMED, "cirrhosis", "25064002");
        table.insert_simple(OntologySource::SNOMED, "malignant_neoplasm", "363346000");
        table.insert_simple(OntologySource::SNOMED, "breast_cancer", "254837009");
        table.insert_simple(OntologySource::SNOMED, "lung_cancer", "254637007");
        table.insert_simple(OntologySource::SNOMED, "colorectal_cancer", "93880001");
        table.insert_simple(OntologySource::SNOMED, "prostate_cancer", "399066001");

        // UBERON anatomy mappings
        table.insert_simple(OntologySource::UBERON, "kidney", "UBERON:0002113");
        table.insert_simple(OntologySource::UBERON, "liver", "UBERON:0002107");
        table.insert_simple(OntologySource::UBERON, "heart", "UBERON:0000948");
        table.insert_simple(OntologySource::UBERON, "lung", "UBERON:0002048");
        table.insert_simple(OntologySource::UBERON, "pancreas", "UBERON:0001264");

        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_resolve_len() {
        let mut table = MappingTable::new();
        assert!(table.is_empty());
        table.insert_simple(OntologySource::DbSNP, "1:100:A:T", "rs123");
        table.insert_simple(OntologySource::HGNC, "BRCA1", "HGNC:1100");
        assert_eq!(table.len(), 2);
        assert_eq!(
            table.resolve(OntologySource::DbSNP, "1:100:A:T"),
            Some("rs123")
        );
        assert_eq!(
            table.resolve(OntologySource::HGNC, "BRCA1"),
            Some("HGNC:1100")
        );
        assert_eq!(table.resolve(OntologySource::DbSNP, "missing"), None);
    }

    #[test]
    fn insert_overwrites() {
        let mut table = MappingTable::new();
        table.insert_simple(OntologySource::DbSNP, "1:100:A:T", "rs123");
        table.insert_simple(OntologySource::DbSNP, "1:100:A:T", "rs999");
        assert_eq!(table.len(), 1);
        assert_eq!(
            table.resolve(OntologySource::DbSNP, "1:100:A:T"),
            Some("rs999")
        );
    }

    #[test]
    fn serde_round_trip() {
        let mut table = MappingTable::new();
        table.insert_simple(OntologySource::DbSNP, "1:100:A:T", "rs123");
        let json = serde_json::to_string(&table).unwrap();
        let decoded: MappingTable = serde_json::from_str(&json).unwrap();
        assert_eq!(
            decoded.resolve(OntologySource::DbSNP, "1:100:A:T"),
            Some("rs123")
        );
    }

    #[test]
    fn resolve_is_readonly_to_map() {
        let table = MappingTable::new();
        let _ = table.resolve(OntologySource::DbSNP, "nope");
        assert!(table.is_empty());
    }

    #[test]
    fn test_ontology_source_display() {
        assert_eq!(OntologySource::DbSNP.to_string(), "dbSNP");
        assert_eq!(OntologySource::HGNC.to_string(), "HGNC");
        assert_eq!(OntologySource::LOINC.to_string(), "LOINC");
        assert_eq!(OntologySource::SNOMED.to_string(), "SNOMED");
        assert_eq!(OntologySource::UBERON.to_string(), "UBERON");
    }

    #[test]
    fn test_ontology_source_from_str() {
        assert_eq!(
            "dbsnp".parse::<OntologySource>().unwrap(),
            OntologySource::DbSNP
        );
        assert_eq!(
            "hgnc".parse::<OntologySource>().unwrap(),
            OntologySource::HGNC
        );
        assert_eq!(
            "loinc".parse::<OntologySource>().unwrap(),
            OntologySource::LOINC
        );
        assert_eq!(
            "snomed".parse::<OntologySource>().unwrap(),
            OntologySource::SNOMED
        );
        assert_eq!(
            "uberon".parse::<OntologySource>().unwrap(),
            OntologySource::UBERON
        );
    }

    #[test]
    fn test_mapping_entry_confidence() {
        let entry = MappingEntry::new(OntologySource::DbSNP, "rs123")
            .with_confidence(0.95)
            .with_notes("High confidence mapping");
        assert_eq!(entry.confidence, 0.95);
        assert_eq!(entry.notes, Some("High confidence mapping".to_string()));
    }

    #[test]
    fn test_get_by_source() {
        let mut table = MappingTable::new();
        table.insert_simple(OntologySource::DbSNP, "1:100:A:T", "rs123");
        table.insert_simple(OntologySource::DbSNP, "1:200:C:G", "rs456");
        table.insert_simple(OntologySource::HGNC, "BRCA1", "HGNC:1100");

        let dbsnp_mappings = table.get_by_source(OntologySource::DbSNP);
        assert_eq!(dbsnp_mappings.len(), 2);

        let hgnc_mappings = table.get_by_source(OntologySource::HGNC);
        assert_eq!(hgnc_mappings.len(), 1);
    }

    #[test]
    fn test_seed_phase1_phase2() {
        let table = MappingTable::seed_phase1_phase2();
        assert!(!table.is_empty());

        // Check dbSNP mappings
        assert_eq!(
            table.resolve(OntologySource::DbSNP, "1:100:A:T"),
            Some("rs123")
        );
        assert_eq!(
            table.resolve(OntologySource::DbSNP, "17:43044295:G:A"),
            Some("rs113488022")
        );

        // Check HGNC mappings
        assert_eq!(
            table.resolve(OntologySource::HGNC, "BRCA1"),
            Some("HGNC:1100")
        );
        assert_eq!(
            table.resolve(OntologySource::HGNC, "TP53"),
            Some("HGNC:11998")
        );

        // Check LOINC mappings
        assert_eq!(
            table.resolve(OntologySource::LOINC, "creatinine"),
            Some("2160-0")
        );
        assert_eq!(
            table.resolve(OntologySource::LOINC, "hba1c"),
            Some("4548-4")
        );

        // Check SNOMED mappings
        assert_eq!(
            table.resolve(OntologySource::SNOMED, "diabetes_mellitus_type_2"),
            Some("44054006")
        );
        assert_eq!(
            table.resolve(OntologySource::SNOMED, "hypertension"),
            Some("38341003")
        );

        // Check UBERON mappings
        assert_eq!(
            table.resolve(OntologySource::UBERON, "kidney"),
            Some("UBERON:0002113")
        );
        assert_eq!(
            table.resolve(OntologySource::UBERON, "heart"),
            Some("UBERON:0000948")
        );
    }
}
