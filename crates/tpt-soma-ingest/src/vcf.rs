use thiserror::Error;

#[derive(Debug, Error)]
pub enum VcfError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

pub struct VcfParser<'a> {
    path: &'a str,
}

impl<'a> VcfParser<'a> {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }

    pub fn parse(&self) -> Result<VcfRecords, VcfError> {
        let content = std::fs::read_to_string(self.path)?;
        let mut records = Vec::new();
        for line in content.lines() {
            if line.starts_with("##") || line.starts_with("#CHROM") {
                continue;
            }
            records.push(line.to_string());
        }
        Ok(VcfRecords { records })
    }
}

pub struct VcfRecords {
    records: Vec<String>,
}

impl VcfRecords {
    pub fn len(&self) -> usize {
        self.records.len()
    }
}
