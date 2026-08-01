use std::fs::File;
use std::io::{BufReader, Read};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum H5adError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

pub struct AnnDataParser<'a> {
    path: &'a str,
}

impl<'a> AnnDataParser<'a> {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }

    pub fn parse(&self) -> Result<AnnData, H5adError> {
        let file = File::open(self.path)?;
        let mut reader = BufReader::new(file);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let obs_count = buf.len();
        Ok(AnnData { obs_count })
    }
}

pub struct AnnData {
    pub obs_count: usize,
}
