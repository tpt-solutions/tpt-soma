use std::collections::HashMap;

#[derive(Default)]
pub struct VariantAnnotationStore {
    annotations: HashMap<String, VariantAnnotation>,
}

#[derive(Clone)]
pub struct VariantAnnotation {
    pub rsid: Option<String>,
    pub clinvar: Option<String>,
}

impl VariantAnnotationStore {
    pub fn annotate(&mut self, variant_id: &str, annotation: VariantAnnotation) {
        self.annotations.insert(variant_id.to_string(), annotation);
    }

    pub fn get(&self, variant_id: &str) -> Option<&VariantAnnotation> {
        self.annotations.get(variant_id)
    }
}
