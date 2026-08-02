use crate::annotation::{Harmonizer, VariantAnnotation, VariantAnnotationStore};
use sqlx::PgPool;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("annotation error: {0}")]
    Annotation(String),
    #[error("storage error: {0}")]
    Storage(String),
}

use thiserror::Error;

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
    harmonizer: Harmonizer,
    annotation_store: VariantAnnotationStore,
    pool: Option<PgPool>,
    sample_id: Option<String>,
}

impl VariantHarmonizationPipeline {
    pub fn new() -> Self {
        Self {
            pipeline: Pipeline::new("variant_harmonization")
                .add_step("parse_vcf")
                .add_step("harmonize_identifiers")
                .add_step("annotate_rsid_clinvar")
                .add_step("store_variants"),
            harmonizer: Harmonizer::new(),
            annotation_store: VariantAnnotationStore::new(),
            pool: None,
            sample_id: None,
        }
    }

    pub fn with_database(mut self, pool: PgPool, sample_id: String) -> Self {
        self.pool = Some(pool);
        self.sample_id = Some(sample_id);
        self
    }

    pub fn steps(&self) -> &[&'static str] {
        self.pipeline.steps()
    }

    pub fn add_rsid_mapping(&mut self, variant_key: String, rsid: String) {
        self.harmonizer.add_rsid_mapping(variant_key, rsid);
    }

    pub fn add_gene_mapping(&mut self, symbol: String, hgnc_id: String) {
        self.harmonizer.add_gene_mapping(symbol, hgnc_id);
    }

    pub async fn run(
        &mut self,
        vcf_path: &str,
    ) -> Result<Vec<(String, VariantAnnotation)>, PipelineError> {
        let records = self.step_parse_vcf(vcf_path)?;
        let mut results = Vec::new();

        for record in records {
            let rsid = self
                .harmonizer
                .harmonize_variant(&record.variant_id)
                .or(record.rsid.clone());

            let annotation = VariantAnnotation {
                rsid: rsid.clone(),
                clinvar: None,
            };

            self.annotation_store
                .annotate(&record.variant_id, annotation.clone());
            results.push((record.variant_id.clone(), annotation));

            // Step: store_variants - store to database if pool is available
            if let (Some(pool), Some(sample_id)) = (&self.pool, &self.sample_id) {
                self.step_store_variants(pool, sample_id, &record, &rsid)
                    .await?;
            }
        }

        Ok(results)
    }

    fn step_parse_vcf(
        &self,
        vcf_path: &str,
    ) -> Result<Vec<tpt_soma_ingest::vcf::VariantRecord>, PipelineError> {
        tpt_soma_ingest::vcf::VcfParser::new(vcf_path)
            .parse()
            .map_err(|e| PipelineError::Parse(e.to_string()))
    }

    async fn step_store_variants(
        &self,
        pool: &PgPool,
        sample_id: &str,
        record: &tpt_soma_ingest::vcf::VariantRecord,
        rsid: &Option<String>,
    ) -> Result<(), PipelineError> {
        // Insert variant if not exists
        sqlx::query(
            r#"
            INSERT INTO variants (variant_id, chromosome, position, reference, alternate, rsid, clinvar_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (variant_id) DO UPDATE SET
                rsid = EXCLUDED.rsid,
                clinvar_id = EXCLUDED.clinvar_id
            "#,
        )
        .bind(&record.variant_id)
        .bind(&record.chromosome)
        .bind(record.position)
        .bind(&record.reference)
        .bind(&record.alternate)
        .bind(rsid)
        .bind(&Option::<String>::None) // clinvar_id - not available yet
        .execute(pool)
        .await
        .map_err(|e| PipelineError::Storage(e.to_string()))?;

        // Insert sample-variant association
        sqlx::query(
            r#"
            INSERT INTO sample_variants (sample_id, variant_id, genotype)
            VALUES ($1, $2, $3)
            ON CONFLICT (sample_id, variant_id) DO UPDATE SET
                genotype = EXCLUDED.genotype
            "#,
        )
        .bind(sample_id)
        .bind(&record.variant_id)
        .bind(&Option::<String>::None) // genotype - not parsed from VCF yet
        .execute(pool)
        .await
        .map_err(|e| PipelineError::Storage(e.to_string()))?;

        Ok(())
    }

    pub fn annotations(&self) -> &VariantAnnotationStore {
        &self.annotation_store
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
