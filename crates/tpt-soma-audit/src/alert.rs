//! Alerting for audit-chain integrity failures.
//!
//! `verify_chain` only reports; this module turns a failed [`ChainReport`] into
//! an [`IntegrityAlert`] and hands it to one or more [`AlertSink`]s. An alert
//! carries event ids and hashes only, never ledger payloads.

use crate::integrity::{ChainFailureKind, ChainReport, IntegrityError};
use crate::ledger::AuditLedger;

/// A detected ledger integrity failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityAlert {
    pub event_id: uuid::Uuid,
    pub index: usize,
    pub kind: ChainFailureKind,
    pub events_checked: usize,
}

impl IntegrityAlert {
    /// One-line description for logs and notifications.
    pub fn message(&self) -> String {
        let what = match self.kind {
            ChainFailureKind::HashMismatch => "hash mismatch (event contents altered)",
            ChainFailureKind::ChainBreak => "chain break (event removed or reordered)",
        };
        format!(
            "audit ledger integrity failure: {what} at event {} (id {}) of {} checked",
            self.index, self.event_id, self.events_checked
        )
    }
}

/// Build an alert from a report, or `None` when the chain is valid.
pub fn alert_for(report: &ChainReport) -> Option<IntegrityAlert> {
    let failure = report.failure.as_ref()?;
    Some(IntegrityAlert {
        event_id: failure.event_id,
        index: failure.index,
        kind: failure.kind,
        events_checked: report.events_checked,
    })
}

/// Destination for integrity alerts.
pub trait AlertSink: Send + Sync {
    fn alert(&self, alert: &IntegrityAlert);
}

/// Emits the alert as a `tracing` error event.
pub struct TracingAlertSink;

impl AlertSink for TracingAlertSink {
    fn alert(&self, alert: &IntegrityAlert) {
        tracing::error!(
            event_id = %alert.event_id,
            index = alert.index,
            "{}",
            alert.message()
        );
    }
}

/// Verify the ledger and notify every sink if the chain is invalid.
/// Returns the report either way so callers can still act on it.
pub async fn check_and_alert(
    ledger: &AuditLedger,
    sinks: &[&dyn AlertSink],
) -> Result<ChainReport, IntegrityError> {
    let report = ledger.verify_chain().await?;
    if let Some(alert) = alert_for(&report) {
        for sink in sinks {
            sink.alert(&alert);
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::{ChainFailure, verify_chain_in_memory};
    use std::sync::Mutex;

    struct Collect(Mutex<Vec<IntegrityAlert>>);

    impl AlertSink for Collect {
        fn alert(&self, alert: &IntegrityAlert) {
            self.0.lock().unwrap().push(alert.clone());
        }
    }

    #[test]
    fn test_valid_report_raises_no_alert() {
        let report = verify_chain_in_memory(&[]);
        assert!(alert_for(&report).is_none());
    }

    #[test]
    fn test_failed_report_becomes_alert() {
        let id = uuid::Uuid::new_v4();
        let report = ChainReport {
            tail_hash: None,
            valid: false,
            events_checked: 7,
            failure: Some(ChainFailure {
                index: 3,
                event_id: id,
                kind: ChainFailureKind::ChainBreak,
                expected_hash: "a".to_string(),
                actual_hash: "b".to_string(),
            }),
        };
        let alert = alert_for(&report).unwrap();
        assert_eq!(alert.event_id, id);
        assert_eq!(alert.index, 3);
        assert_eq!(alert.events_checked, 7);
        assert!(alert.message().contains("chain break"));
        assert!(alert.message().contains(&id.to_string()));

        let sink = Collect(Mutex::new(Vec::new()));
        sink.alert(&alert);
        assert_eq!(sink.0.lock().unwrap().len(), 1);
    }
}
