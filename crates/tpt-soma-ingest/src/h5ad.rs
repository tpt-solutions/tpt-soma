use std::fs::File;
use std::io::{BufReader, Read};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum H5adError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("anndata error: {0}")]
    AnnData(String),
}

pub struct AnnDataParser<'a> {
    path: &'a str,
}

impl<'a> AnnDataParser<'a> {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }

    pub fn parse(&self) -> Result<AnnDataResult, H5adError> {
        let file = File::open(self.path)?;
        let mut reader = BufReader::new(file);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let obs_count = buf.len();
        Ok(AnnDataResult {
            metadata: ScRNASeqMetadata {
                sample_id: "unknown".to_string(),
                n_cells: obs_count,
                n_genes: 0,
                cell_ids: Vec::new(),
                gene_ids: Vec::new(),
            },
            records: Vec::new(),
        })
    }
}

pub struct AnnDataResult {
    pub metadata: ScRNASeqMetadata,
    pub records: Vec<ScRNASeqRecord>,
}

#[derive(Debug, Clone)]
pub struct ScRNASeqRecord {
    pub sample_id: String,
    pub cell_id: String,
    pub gene_id: String,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct ScRNASeqMetadata {
    pub sample_id: String,
    pub n_cells: usize,
    pub n_genes: usize,
    pub cell_ids: Vec<String>,
    pub gene_ids: Vec<String>,
}

impl AnnDataResult {
    pub fn n_cells(&self) -> usize {
        self.metadata.n_cells
    }

    pub fn n_genes(&self) -> usize {
        self.metadata.n_genes
    }

    pub fn n_records(&self) -> usize {
        self.records.len()
    }
}
