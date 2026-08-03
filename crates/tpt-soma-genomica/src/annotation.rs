use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct VariantAnnotation {
    pub rsid: Option<String>,
    pub clinvar: Option<String>,
}

#[derive(Default)]
pub struct VariantAnnotationStore {
    annotations: HashMap<String, VariantAnnotation>,
}

impl VariantAnnotationStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn annotate(&mut self, variant_id: &str, annotation: VariantAnnotation) {
        self.annotations.insert(variant_id.to_string(), annotation);
    }

    pub fn get(&self, variant_id: &str) -> Option<&VariantAnnotation> {
        self.annotations.get(variant_id)
    }

    pub fn len(&self) -> usize {
        self.annotations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.annotations.is_empty()
    }
}

pub struct Harmonizer {
    rsid_map: HashMap<String, String>,
    gene_symbol_map: HashMap<String, String>,
}

impl Harmonizer {
    pub fn new() -> Self {
        Self {
            rsid_map: HashMap::new(),
            gene_symbol_map: HashMap::new(),
        }
    }

    pub fn add_rsid_mapping(&mut self, variant_key: String, rsid: String) {
        self.rsid_map.insert(variant_key, rsid);
    }

    pub fn add_gene_mapping(&mut self, symbol: String, hgnc_id: String) {
        self.gene_symbol_map.insert(symbol, hgnc_id);
    }

    pub fn harmonize_variant(&self, variant_key: &str) -> Option<String> {
        self.rsid_map.get(variant_key).cloned()
    }

    pub fn harmonize_gene(&self, symbol: &str) -> Option<String> {
        self.gene_symbol_map.get(symbol).cloned()
    }
}

impl Default for Harmonizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harmonizer_rsid() {
        let mut h = Harmonizer::new();
        h.add_rsid_mapping("1:100:A:T".to_string(), "rs123".to_string());
        assert_eq!(h.harmonize_variant("1:100:A:T"), Some("rs123".to_string()));
        assert_eq!(h.harmonize_variant("1:200:C:G"), None);
    }

    #[test]
    fn test_harmonizer_gene() {
        let mut h = Harmonizer::new();
        h.add_gene_mapping("BRCA1".to_string(), "HGNC:1100".to_string());
        assert_eq!(h.harmonize_gene("BRCA1"), Some("HGNC:1100".to_string()));
        assert_eq!(h.harmonize_gene("TP53"), None);
    }

    #[test]
    fn test_annotation_store_get_missing_is_none() {
        let store = VariantAnnotationStore::new();
        assert!(store.is_empty());
        assert_eq!(store.get("1:100:A:T"), None);
    }

    #[test]
    fn test_annotation_store_overwrite_and_count() {
        let mut store = VariantAnnotationStore::new();
        store.annotate(
            "1:100:A:T",
            VariantAnnotation {
                rsid: Some("rs123".to_string()),
                clinvar: None,
            },
        );
        store.annotate(
            "1:100:A:T",
            VariantAnnotation {
                rsid: Some("rs999".to_string()),
                clinvar: Some("VCV000999".to_string()),
            },
        );
        store.annotate(
            "2:300:T:A",
            VariantAnnotation {
                rsid: None,
                clinvar: None,
            },
        );
        assert_eq!(store.len(), 2);
        let annotation = store.get("1:100:A:T").unwrap();
        assert_eq!(annotation.rsid.as_deref(), Some("rs999"));
        assert_eq!(annotation.clinvar.as_deref(), Some("VCV000999"));
    }

    #[test]
    fn test_harmonizer_default_is_empty() {
        let h = Harmonizer::default();
        assert_eq!(h.harmonize_variant("1:100:A:T"), None);
        assert_eq!(h.harmonize_gene("BRCA1"), None);
    }
}
