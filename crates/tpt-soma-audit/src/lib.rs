pub mod alert;
pub mod compliance;
pub mod integrity;
pub mod ledger;

pub use alert::{AlertSink, IntegrityAlert, TracingAlertSink, alert_for, check_and_alert};
pub use compliance::{ComplianceEntry, ComplianceError, ComplianceReporter};
pub use integrity::{ChainFailure, ChainFailureKind, ChainReport, IntegrityError, verify_chain};
pub use ledger::{AuditError, AuditEvent, AuditLedger};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
