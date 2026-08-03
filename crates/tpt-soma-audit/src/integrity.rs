use crate::ledger::{AuditEvent, AuditLedger};
use sha2::{Digest, Sha256};

pub async fn verify_chain(ledger: &AuditLedger) -> Result<ChainReport, IntegrityError> {
    let pool = &ledger.pool;

    // Get all events in order
    let rows = sqlx::query_as::<_, AuditEvent>(
        r#"
        SELECT id, actor, resource_class, action, cohort_scope, timestamp, 
               query_fingerprint, outcome, prev_row_hash, row_hash
        FROM audit_ledger
        ORDER BY timestamp ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(ChainReport {
            tail_hash: None,
            valid: true,
            events_checked: 0,
        });
    }

    let mut prev_hash: Option<String> = None;
    let mut valid = true;

    for (i, event) in rows.iter().enumerate() {
        // Recompute the expected hash
        let mut hasher = Sha256::new();
        if let Some(prev) = &prev_hash {
            hasher.update(prev.as_bytes());
        }
        let payload = serde_json::json!({
            "id": event.id,
            "actor": event.actor,
            "resource_class": event.resource_class,
            "action": event.action,
            "cohort_scope": event.cohort_scope,
            "timestamp": event.timestamp,
            "query_fingerprint": event.query_fingerprint,
            "outcome": event.outcome,
        });
        hasher.update(serde_json::to_vec(&payload).unwrap());
        let expected_hash = format!("{:x}", hasher.finalize());

        // Check if the stored hash matches
        if event.row_hash != expected_hash {
            eprintln!("Hash mismatch at event {} (id: {})", i, event.id);
            eprintln!("  Expected: {}", expected_hash);
            eprintln!("  Got:      {}", event.row_hash);
            valid = false;
            break;
        }

        // Check if prev_row_hash matches previous event's row_hash
        if event.prev_row_hash != prev_hash {
            eprintln!("Chain break at event {} (id: {})", i, event.id);
            eprintln!("  Expected prev: {:?}", prev_hash);
            eprintln!("  Got prev:      {:?}", event.prev_row_hash);
            valid = false;
            break;
        }

        prev_hash = Some(event.row_hash.clone());
    }

    Ok(ChainReport {
        tail_hash: prev_hash,
        valid,
        events_checked: rows.len(),
    })
}

/// Verify a chain of audit events in-memory (for testing without database)
pub fn verify_chain_in_memory(events: &[AuditEvent]) -> ChainReport {
    if events.is_empty() {
        return ChainReport {
            tail_hash: None,
            valid: true,
            events_checked: 0,
        };
    }

    let mut prev_hash: Option<String> = None;
    let mut valid = true;

    for (i, event) in events.iter().enumerate() {
        // Recompute the expected hash
        let mut hasher = Sha256::new();
        if let Some(prev) = &prev_hash {
            hasher.update(prev.as_bytes());
        }
        let payload = serde_json::json!({
            "id": event.id,
            "actor": event.actor,
            "resource_class": event.resource_class,
            "action": event.action,
            "cohort_scope": event.cohort_scope,
            "timestamp": event.timestamp,
            "query_fingerprint": event.query_fingerprint,
            "outcome": event.outcome,
        });
        hasher.update(serde_json::to_vec(&payload).unwrap());
        let expected_hash = format!("{:x}", hasher.finalize());

        // Check if the stored hash matches
        if event.row_hash != expected_hash {
            eprintln!("Hash mismatch at event {} (id: {})", i, event.id);
            eprintln!("  Expected: {}", expected_hash);
            eprintln!("  Got:      {}", event.row_hash);
            valid = false;
            break;
        }

        // Check if prev_row_hash matches previous event's row_hash
        if event.prev_row_hash != prev_hash {
            eprintln!("Chain break at event {} (id: {})", i, event.id);
            eprintln!("  Expected prev: {:?}", prev_hash);
            eprintln!("  Got prev:      {:?}", event.prev_row_hash);
            valid = false;
            break;
        }

        prev_hash = Some(event.row_hash.clone());
    }

    ChainReport {
        tail_hash: prev_hash,
        valid,
        events_checked: events.len(),
    }
}

pub struct ChainReport {
    pub tail_hash: Option<String>,
    pub valid: bool,
    pub events_checked: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum IntegrityError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("hash mismatch")]
    HashMismatch,
    #[error("chain break detected")]
    ChainBreak,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    fn make_test_event(
        id: Uuid,
        actor: &str,
        resource_class: &str,
        action: &str,
        cohort_scope: Vec<String>,
        timestamp: DateTime<Utc>,
        query_fingerprint: &str,
        outcome: &str,
        prev_row_hash: Option<String>,
    ) -> AuditEvent {
        let mut hasher = Sha256::new();
        if let Some(prev) = &prev_row_hash {
            hasher.update(prev.as_bytes());
        }
        let payload = serde_json::json!({
            "id": id,
            "actor": actor,
            "resource_class": resource_class,
            "action": action,
            "cohort_scope": cohort_scope,
            "timestamp": timestamp,
            "query_fingerprint": query_fingerprint,
            "outcome": outcome,
        });
        hasher.update(serde_json::to_vec(&payload).unwrap());
        let row_hash = format!("{:x}", hasher.finalize());

        AuditEvent {
            id,
            actor: actor.to_string(),
            resource_class: resource_class.to_string(),
            action: action.to_string(),
            cohort_scope,
            timestamp,
            query_fingerprint: query_fingerprint.to_string(),
            outcome: outcome.to_string(),
            prev_row_hash,
            row_hash,
        }
    }

    #[test]
    fn test_verify_chain_empty() {
        let events: Vec<AuditEvent> = vec![];
        let report = verify_chain_in_memory(&events);
        assert!(report.valid);
        assert_eq!(report.events_checked, 0);
        assert_eq!(report.tail_hash, None);
    }

    #[test]
    fn test_verify_chain_valid() {
        let now = Utc::now();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let event1 = make_test_event(
            id1,
            "researcher-1",
            "genomic_variant",
            "read",
            vec!["cohort-a".to_string()],
            now,
            "query-1",
            "success",
            None,
        );
        let event2 = make_test_event(
            id2,
            "researcher-2",
            "transcriptomic_scrna",
            "read",
            vec!["cohort-a".to_string()],
            now,
            "query-2",
            "success",
            Some(event1.row_hash.clone()),
        );
        let event3 = make_test_event(
            id3,
            "researcher-1",
            "genomic_variant",
            "read",
            vec!["cohort-b".to_string()],
            now,
            "query-3",
            "success",
            Some(event2.row_hash.clone()),
        );

        let expected_tail_hash = event3.row_hash.clone();
        let events = vec![event1, event2, event3];
        let report = verify_chain_in_memory(&events);
        assert!(report.valid);
        assert_eq!(report.events_checked, 3);
        assert_eq!(report.tail_hash, Some(expected_tail_hash));
    }

    #[test]
    fn test_verify_chain_tampered_hash() {
        let now = Utc::now();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let mut event1 = make_test_event(
            id1,
            "researcher-1",
            "genomic_variant",
            "read",
            vec!["cohort-a".to_string()],
            now,
            "query-1",
            "success",
            None,
        );
        let event2 = make_test_event(
            id2,
            "researcher-2",
            "transcriptomic_scrna",
            "read",
            vec!["cohort-a".to_string()],
            now,
            "query-2",
            "success",
            Some(event1.row_hash.clone()),
        );

        // Tamper with event1's row_hash
        event1.row_hash = "tampered_hash".to_string();

        let events = vec![event1, event2];
        let report = verify_chain_in_memory(&events);
        assert!(!report.valid);
        assert_eq!(report.events_checked, 2);
    }

    #[test]
    fn test_verify_chain_tampered_prev_hash() {
        let now = Utc::now();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let event1 = make_test_event(
            id1,
            "researcher-1",
            "genomic_variant",
            "read",
            vec!["cohort-a".to_string()],
            now,
            "query-1",
            "success",
            None,
        );
        let mut event2 = make_test_event(
            id2,
            "researcher-2",
            "transcriptomic_scrna",
            "read",
            vec!["cohort-a".to_string()],
            now,
            "query-2",
            "success",
            Some(event1.row_hash.clone()),
        );

        // Tamper with event2's prev_row_hash
        event2.prev_row_hash = Some("wrong_prev_hash".to_string());

        let events = vec![event1, event2];
        let report = verify_chain_in_memory(&events);
        assert!(!report.valid);
        assert_eq!(report.events_checked, 2);
    }

    #[test]
    fn test_verify_chain_single_event() {
        let now = Utc::now();
        let id1 = Uuid::new_v4();

        let event1 = make_test_event(
            id1,
            "researcher-1",
            "genomic_variant",
            "read",
            vec!["cohort-a".to_string()],
            now,
            "query-1",
            "success",
            None,
        );

        let expected_tail_hash = event1.row_hash.clone();
        let events = vec![event1];
        let report = verify_chain_in_memory(&events);
        assert!(report.valid);
        assert_eq!(report.events_checked, 1);
        assert_eq!(report.tail_hash, Some(expected_tail_hash));
    }
}
