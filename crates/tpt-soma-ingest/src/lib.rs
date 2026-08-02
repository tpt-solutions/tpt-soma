pub mod endpoint;
pub mod h5ad;
pub mod vcf;

pub use h5ad::{AnnDataParser, AnnDataResult, ScRNASeqMetadata, ScRNASeqRecord};
pub use vcf::VcfParser;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
