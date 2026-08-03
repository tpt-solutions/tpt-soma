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
    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_verify_chain_empty() {
        // Would test with empty database
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_verify_chain_valid() {
        // Would test with valid chain
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_verify_chain_tampered() {
        // Would test with tampered chain
    }
}
