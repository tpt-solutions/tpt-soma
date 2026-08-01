pub mod token;
pub mod attenuation;
pub mod registry;
pub mod revocation;

pub use token::CapabilityToken;
pub use registry::DataClassRegistry;
pub use revocation::RevocationList;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;