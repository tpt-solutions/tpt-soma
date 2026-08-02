use noodles::vcf;
use noodles::vcf::variant::record::{AlternateBases, Ids};
use std::fs::File;
use std::io::BufReader;
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
        let mut reader = BufReader::new(file);
        let mut vcf_reader = vcf::io::Reader::new(&mut reader);

        let _header = vcf_reader
            .read_header()
            .map_err(|e| VcfError::Parse(e.to_string()))?;

        let mut records = Vec::new();

        for result in vcf_reader.records() {
            let record = result.map_err(|e| VcfError::Parse(e.to_string()))?;
            let chromosome = record.reference_sequence_name().to_string();
            let position = match record.variant_start() {
                Some(Ok(pos)) => usize::from(pos) as i32,
                Some(Err(e)) => return Err(VcfError::Parse(e.to_string())),
                None => 0,
            };
            let reference = record.reference_bases().to_string();
            let alternate_bases: Vec<String> = record
                .alternate_bases()
                .iter()
                .map(|r| r.unwrap_or_default().to_string())
                .collect();
            let alternate = alternate_bases.join(",");
            let rsid = record.ids().iter().next().map(|s| s.to_string());

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
