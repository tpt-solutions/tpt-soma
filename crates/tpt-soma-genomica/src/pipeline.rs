use crate::annotation::VariantAnnotation;

pub struct Pipeline {
    pub name: String,
    pub steps: Vec<&'static str>,
}

impl Pipeline {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    pub fn add_step(mut self, step: &'static str) -> Self {
        self.steps.push(step);
        self
    }

    pub fn steps(&self) -> &[&'static str] {
        &self.steps
    }
}

pub struct VariantHarmonizationPipeline {
    pipeline: Pipeline,
}

impl VariantHarmonizationPipeline {
    pub fn new() -> Self {
        Self {
            pipeline: Pipeline::new("variant_harmonization")
                .add_step("parse_vcf")
                .add_step("harmonize_identifiers")
                .add_step("annotate_rsid_clinvar")
                .add_step("store_variants"),
        }
    }

    pub fn steps(&self) -> &[&'static str] {
        self.pipeline.steps()
    }
}

impl Default for VariantHarmonizationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_steps() {
        let pipeline = VariantHarmonizationPipeline::new();
        assert_eq!(pipeline.steps().len(), 4);
        assert_eq!(pipeline.steps()[0], "parse_vcf");
        assert_eq!(pipeline.steps()[3], "store_variants");
    }
}