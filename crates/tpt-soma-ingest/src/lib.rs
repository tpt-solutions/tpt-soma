pub mod vcf;
pub mod h5ad;
pub mod endpoint;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
