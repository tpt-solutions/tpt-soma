-- scRNA-seq UMAP coordinates and cluster labels
CREATE TABLE scrna_umap (
    sample_id UUID NOT NULL REFERENCES samples(sample_id) ON DELETE CASCADE,
    cell_id TEXT NOT NULL,
    umap1 DOUBLE PRECISION NOT NULL,
    umap2 DOUBLE PRECISION NOT NULL,
    cluster TEXT NOT NULL,
    PRIMARY KEY (sample_id, cell_id)
);

CREATE INDEX idx_scrna_umap_sample ON scrna_umap (sample_id);
CREATE INDEX idx_scrna_umap_cluster ON scrna_umap (cluster);