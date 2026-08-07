-- Phase 4: Ontological Soma Graph (OSG) topology + computational-pathology /
-- clinical-trial relational tables (tpt-soma-pathos, tpt-soma-clinica slices).
--
-- This consolidates the macro-anatomy + signaling-molecule nodes the Phase 4
-- cross-talk solver operates over. Node/edge *types* are declared here; concrete
-- instances (e.g. the adipose -> IGF-1 -> breast-tissue example) are created at
-- runtime by Phase 4 algorithms via the Plexus client.

SELECT plexus.create_node_type('MacroAnatomy', '{
    "type": "object",
    "properties": {
        "uberon_id": {"type": "string"},
        "name": {"type": "string"},
        "system": {"type": "string"}
    },
    "required": ["uberon_id", "name"]
}');

SELECT plexus.create_node_type('SignalingMolecule', '{
    "type": "object",
    "properties": {
        "name": {"type": "string"},
        "symbol": {"type": "string"}
    },
    "required": ["name"]
}');

-- Cross-talk edge: paracrine / endocrine coupling between OSG entities, the edge
-- type the generalized cross-talk solver traverses.
SELECT plexus.create_edge_type('cross_talk', 'MacroAnatomy', 'SignalingMolecule', '{
    "type": "object",
    "properties": {
        "mechanism": {"type": "string"},
        "direction": {"type": "string"},
        "strength": {"type": "number"}
    }
}');

SELECT plexus.create_edge_type('cross_talk', 'SignalingMolecule', 'MacroAnatomy', '{}');
SELECT plexus.create_edge_type('cross_talk', 'MacroAnatomy', 'MacroAnatomy', '{}');

SELECT plexus.create_index('MacroAnatomy', 'uberon_id');

-- Computational pathology findings (tpt-soma-pathos)
CREATE TABLE IF NOT EXISTS pathos_findings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_id TEXT NOT NULL,
    finding_type TEXT NOT NULL,
    detail JSONB NOT NULL,
    risk_score DOUBLE PRECISION,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_pathos_findings_subject ON pathos_findings (subject_id);

-- Clinical trial design / cohort discovery (tpt-soma-clinica slice)
CREATE TABLE IF NOT EXISTS clinical_trial_cohorts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trial_name TEXT NOT NULL,
    cohort_label TEXT NOT NULL,
    inclusion_criteria JSONB NOT NULL DEFAULT '[]',
    exclusion_criteria JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Biomarker discovery / validation results (tpt-soma-clinica slice)
CREATE TABLE IF NOT EXISTS biomarker_discovery (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    analysis_name TEXT NOT NULL,
    biomarker TEXT NOT NULL,
    statistic DOUBLE PRECISION,
    p_value DOUBLE PRECISION,
    result JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Phase 4 data classes (also seeded in tpt-soma-capability registry)
INSERT INTO data_class_registry (id, description, sensitivity) VALUES
    ('pathos_finding', 'Computational pathology findings (insulin-resistance, TME, etc.)', 'Confidential'),
    ('clinical_trial', 'Clinical trial design / cohort-discovery metadata', 'Confidential'),
    ('biomarker_discovery', 'Biomarker discovery/validation statistical outputs', 'Confidential')
ON CONFLICT (id) DO NOTHING;
