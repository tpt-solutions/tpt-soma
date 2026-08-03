-- Phase 2: Chronos time-series extension (CGM + organ function trajectories),
-- Canopy JSON extension (raw FHIR payloads), Plexus Organ/OrganSystem graph
-- extension, and the relational tables backing them.

CREATE EXTENSION IF NOT EXISTS chronos;
CREATE EXTENSION IF NOT EXISTS canopy;

-- Chronos series declaration: continuous glucose monitor readings
SELECT chronos.create_series('cgm_readings', '{
    "type": "object",
    "properties": {
        "subject_id": {"type": "string"},
        "glucose_mgdl": {"type": "number"},
        "source": {"type": "string"},
        "sensor_id": {"type": "string"},
        "is_calibrated": {"type": "boolean"},
        "trend_arrow": {"type": "string"}
    },
    "required": ["subject_id", "glucose_mgdl", "source"]
}', 'ts');

-- Chronos series declaration: organ function test observations (any LOINC-coded measurement)
SELECT chronos.create_series('organ_function_observations', '{
    "type": "object",
    "properties": {
        "subject_id": {"type": "string"},
        "loinc_code": {"type": "string"},
        "value": {"type": "number"},
        "unit": {"type": "string"},
        "status": {"type": "string"},
        "interpretation": {"type": "array", "items": {"type": "string"}},
        "source": {"type": "string"}
    },
    "required": ["subject_id", "loinc_code", "value"]
}', 'effective_time');

-- Backing relational tables (materialized by the Chronos extension; declared here so
-- sqlx query helpers can target them directly, matching the Plexus pattern established
-- in migration 20240101000004)
CREATE TABLE IF NOT EXISTS cgm_readings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_id TEXT NOT NULL,
    ts TIMESTAMPTZ NOT NULL,
    glucose_mgdl DOUBLE PRECISION NOT NULL CHECK (glucose_mgdl > 0),
    source TEXT NOT NULL,
    sensor_id TEXT,
    is_calibrated BOOLEAN NOT NULL DEFAULT false,
    trend_arrow TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (subject_id, ts, source)
);

CREATE INDEX idx_cgm_readings_subject_ts ON cgm_readings (subject_id, ts);

CREATE TABLE IF NOT EXISTS organ_function_observations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_id TEXT NOT NULL,
    loinc_code TEXT NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    unit TEXT,
    effective_time TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'final',
    interpretation TEXT[] NOT NULL DEFAULT '{}',
    source TEXT NOT NULL DEFAULT 'fhir',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_organ_function_obs_subject_loinc ON organ_function_observations (subject_id, loinc_code, effective_time);

-- Canopy: raw FHIR resource payloads stored as JSON alongside the normalized rows above
SELECT canopy.create_document_type('fhir_resource', '{
    "type": "object",
    "properties": {
        "resource_type": {"type": "string"},
        "resource_id": {"type": "string"},
        "payload": {"type": "object"}
    },
    "required": ["resource_type", "resource_id", "payload"]
}');

CREATE TABLE IF NOT EXISTS fhir_resource_payloads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    payload JSONB NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (resource_type, resource_id)
);

CREATE INDEX idx_fhir_resource_payloads_type ON fhir_resource_payloads (resource_type);

-- Organ imaging metadata (pixel data lives in MinIO; this row is the Keystone-side index)
CREATE TABLE IF NOT EXISTS organ_imaging_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_id TEXT NOT NULL,
    study_instance_uid TEXT NOT NULL,
    series_instance_uid TEXT NOT NULL,
    sop_instance_uid TEXT NOT NULL,
    modality TEXT NOT NULL,
    body_part_examined TEXT,
    organ_system TEXT, -- UBERON code
    laterality TEXT,
    minio_bucket TEXT NOT NULL,
    minio_object_key TEXT NOT NULL,
    checksum_sha256 TEXT NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (study_instance_uid, series_instance_uid, sop_instance_uid)
);

CREATE INDEX idx_organ_imaging_subject ON organ_imaging_records (subject_id);

-- Plexus graph extension: Organ, OrganSystem nodes; cross_organ_coupling edges
SELECT plexus.create_node_type('Organ', '{
    "type": "object",
    "properties": {
        "uberon_id": {"type": "string"},
        "name": {"type": "string"},
        "system": {"type": "string"},
        "functions": {"type": "array", "items": {"type": "string"}},
        "biomarkers": {"type": "array", "items": {"type": "string"}}
    },
    "required": ["uberon_id", "name", "system"]
}');

SELECT plexus.create_node_type('OrganSystem', '{
    "type": "object",
    "properties": {
        "system_id": {"type": "string"},
        "name": {"type": "string"}
    },
    "required": ["system_id", "name"]
}');

SELECT plexus.create_edge_type('cross_organ_coupling', 'Organ', 'Organ', '{
    "type": "object",
    "properties": {
        "coupling_type": {"type": "string"},
        "strength": {"type": "number"},
        "mediators": {"type": "array", "items": {"type": "string"}},
        "evidence_level": {"type": "string"},
        "description": {"type": "string"}
    },
    "required": ["coupling_type", "strength"]
}');

SELECT plexus.create_edge_type('belongs_to_system', 'Organ', 'OrganSystem', '{}');

SELECT plexus.create_index('Organ', 'uberon_id');
SELECT plexus.create_index('OrganSystem', 'system_id');

-- Phase 2 data classes
INSERT INTO data_class_registry (id, description, sensitivity) VALUES
    ('clinical_observation', 'Normalized clinical observations from FHIR/CSV ingestion', 'Confidential'),
    ('cgm_continuous', 'Continuous glucose monitor readings', 'Confidential'),
    ('organ_imaging', 'Organ imaging metadata (MRI/CT/ultrasound/PET)', 'Restricted')
ON CONFLICT (id) DO NOTHING;
