pub mod token;
pub mod attenuation;
pub mod registry;
pub mod revocation;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
