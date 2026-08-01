-- Phase 1: Core relational tables for samples, cohorts, and data classes

CREATE TABLE samples (
    sample_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    patient_id UUID,
    source TEXT NOT NULL CHECK (source IN ('public', 'patient')),
    dataset_provenance TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE cohorts (
    cohort_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE cohort_membership (
    cohort_id UUID NOT NULL REFERENCES cohorts(cohort_id) ON DELETE CASCADE,
    sample_id UUID NOT NULL REFERENCES samples(sample_id) ON DELETE CASCADE,
    PRIMARY KEY (cohort_id, sample_id)
);

CREATE TABLE data_class_registry (
    id TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    sensitivity TEXT NOT NULL CHECK (sensitivity IN ('Public', 'Internal', 'Confidential', 'Restricted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Insert Phase 0 seed data classes
INSERT INTO data_class_registry (id, description, sensitivity) VALUES
    ('genomic_raw', 'Raw sequencing reads', 'Restricted'),
    ('genomic_variant', 'Variant calls (VCF)', 'Confidential'),
    ('transcriptomic_scrna', 'Single-cell RNA-seq', 'Confidential'),
    ('phi_demographic', 'PHI demographic data', 'Restricted')
ON CONFLICT (id) DO NOTHING;

-- Plexus graph schema for Phase 1: Gene, Variant, ProteinInteraction nodes
-- These are created via Keystone's Plexus extension DDL
-- The actual graph tables are managed by Plexus, but we define the node/edge types here for reference

-- scRNA-seq expression matrix (sparse storage: sample_id, cell_id, gene_id, count)
CREATE TABLE scrna_expression (
    sample_id UUID NOT NULL REFERENCES samples(sample_id) ON DELETE CASCADE,
    cell_id TEXT NOT NULL,
    gene_id TEXT NOT NULL,
    count INTEGER NOT NULL CHECK (count >= 0),
    PRIMARY KEY (sample_id, cell_id, gene_id)
);

CREATE INDEX idx_scrna_expression_sample ON scrna_expression (sample_id);
CREATE INDEX idx_scrna_expression_gene ON scrna_expression (gene_id);
CREATE INDEX idx_scrna_expression_cell ON scrna_expression (cell_id);

-- Variant table for harmonized variants
CREATE TABLE variants (
    variant_id TEXT PRIMARY KEY,
    chromosome TEXT NOT NULL,
    position INTEGER NOT NULL,
    reference TEXT NOT NULL,
    alternate TEXT NOT NULL,
    rsid TEXT,
    clinvar_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_variants_chrom_pos ON variants (chromosome, position);
CREATE INDEX idx_variants_rsid ON variants (rsid);

-- Sample-variant association (which samples have which variants)
CREATE TABLE sample_variants (
    sample_id UUID NOT NULL REFERENCES samples(sample_id) ON DELETE CASCADE,
    variant_id TEXT NOT NULL REFERENCES variants(variant_id) ON DELETE CASCADE,
    genotype TEXT, -- e.g., "0/1", "1/1"
    PRIMARY KEY (sample_id, variant_id)
);

CREATE INDEX idx_sample_variants_sample ON sample_variants (sample_id);
CREATE INDEX idx_sample_variants_variant ON sample_variants (variant_id);