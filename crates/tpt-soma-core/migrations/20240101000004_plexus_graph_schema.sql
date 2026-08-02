-- Phase 1: Plexus graph schema for Gene, Variant, ProteinInteraction nodes
-- These are created via Keystone's Plexus extension DDL

-- Enable Plexus extension if not already enabled
CREATE EXTENSION IF NOT EXISTS plexus;

-- Gene nodes
SELECT plexus.create_node_type('Gene', '{
    "type": "object",
    "properties": {
        "gene_id": {"type": "string"},
        "symbol": {"type": "string"},
        "name": {"type": "string"},
        "chromosome": {"type": "string"},
        "start": {"type": "integer"},
        "end": {"type": "integer"},
        "strand": {"type": "string"},
        "biotype": {"type": "string"}
    },
    "required": ["gene_id", "symbol"]
}');

-- Variant nodes
SELECT plexus.create_node_type('Variant', '{
    "type": "object",
    "properties": {
        "variant_id": {"type": "string"},
        "chromosome": {"type": "string"},
        "position": {"type": "integer"},
        "reference": {"type": "string"},
        "alternate": {"type": "string"},
        "rsid": {"type": "string"},
        "clinvar_id": {"type": "string"},
        "consequence": {"type": "string"}
    },
    "required": ["variant_id", "chromosome", "position", "reference", "alternate"]
}');

-- ProteinInteraction nodes
SELECT plexus.create_node_type('ProteinInteraction', '{
    "type": "object",
    "properties": {
        "interaction_id": {"type": "string"},
        "source": {"type": "string"},
        "confidence": {"type": "number"},
        "evidence": {"type": "array", "items": {"type": "string"}}
    },
    "required": ["interaction_id", "source"]
}');

-- Edge: Gene harbors Variant
SELECT plexus.create_edge_type('harbors_variant', 'Gene', 'Variant', '{
    "type": "object",
    "properties": {
        "zygosity": {"type": "string"},
        "genotype": {"type": "string"}
    }
}');

-- Edge: Gene interacts with Gene (via ProteinInteraction)
SELECT plexus.create_edge_type('interacts_with', 'Gene', 'Gene', '{
    "type": "object",
    "properties": {
        "interaction_id": {"type": "string"},
        "source": {"type": "string"},
        "confidence": {"type": "number"}
    }
}');

-- Edge: Variant affects Gene
SELECT plexus.create_edge_type('affects', 'Variant', 'Gene', '{
    "type": "object",
    "properties": {
        "consequence": {"type": "string"},
        "impact": {"type": "string"}
    }
}');

-- Create indexes for common query patterns
SELECT plexus.create_index('Gene', 'symbol');
SELECT plexus.create_index('Gene', 'gene_id');
SELECT plexus.create_index('Variant', 'variant_id');
SELECT plexus.create_index('Variant', 'rsid');
SELECT plexus.create_index('Variant', 'chromosome', 'position');
SELECT plexus.create_index('ProteinInteraction', 'interaction_id');