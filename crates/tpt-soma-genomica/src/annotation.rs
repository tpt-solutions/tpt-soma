use std::collections::HashMap;

#[derive(Debug, Clone)]
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
}