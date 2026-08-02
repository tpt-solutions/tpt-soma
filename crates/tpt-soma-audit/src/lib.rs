pub mod compliance;
pub mod integrity;
pub mod ledger;

pub use compliance::{ComplianceEntry, ComplianceError, ComplianceReporter};
pub use integrity::{ChainReport, IntegrityError, verify_chain};
pub use ledger::{AuditError, AuditEvent, AuditLedger};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
