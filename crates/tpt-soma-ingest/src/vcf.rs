use std::fs::File;
use std::io::{BufRead, BufReader};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VcfError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Clone)]
pub struct VariantRecord {
    pub variant_id: String,
    pub chromosome: String,
    pub position: i32,
    pub reference: String,
    pub alternate: String,
    pub rsid: Option<String>,
}

pub struct VcfParser<'a> {
    path: &'a str,
}

impl<'a> VcfParser<'a> {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }

    pub fn parse(&self) -> Result<Vec<VariantRecord>, VcfError> {
        let file = File::open(self.path)?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 5 {
                continue;
            }
            let chromosome = fields[0].to_string();
            let position = fields[1].parse().unwrap_or(0);
            let rsid = if fields[2].starts_with("rs") {
                Some(fields[2].to_string())
            } else {
                None
            };
            let reference = fields[3].to_string();
            let alternate = fields[4].to_string();
            let variant_id = format!("{}:{}:{}:{}", chromosome, position, reference, alternate);

            records.push(VariantRecord {
                variant_id,
                chromosome,
                position,
                reference,
                alternate,
                rsid,
            });
        }

        Ok(records)
    }
}
