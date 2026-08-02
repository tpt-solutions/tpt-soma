pub mod annotation;
pub mod pipeline;

pub use annotation::{Harmonizer, VariantAnnotation, VariantAnnotationStore};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
