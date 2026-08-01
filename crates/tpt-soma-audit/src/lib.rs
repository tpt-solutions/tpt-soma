pub mod ledger;
pub mod integrity;
pub mod compliance;

pub use ledger::{AuditEvent, AuditLedger, AuditError};
pub use integrity::{ChainReport, IntegrityError, verify_chain};
pub use compliance::{ComplianceReporter, ComplianceEntry, ComplianceError};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;