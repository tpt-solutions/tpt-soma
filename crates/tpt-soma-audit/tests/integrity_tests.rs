use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tpt_soma_audit::{AuditEvent, AuditLedger, integrity::verify_chain};
use tpt_soma_core::test_helpers::test_pool;
use uuid::Uuid;

async fn setup_test_ledger() -> (PgPool, AuditLedger) {
    let pool = test_pool()
        .await
        .expect("Failed to connect to test database");
    let ledger = AuditLedger::new(pool.clone());
    (pool, ledger)
}

#[allow(clippy::too_many_arguments)]
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
    use sha2::{Digest, Sha256};
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

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_verify_chain_valid_database() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let (pool, ledger) = setup_test_ledger().await;

    // Clean up any existing data
    sqlx::query("DELETE FROM audit_ledger")
        .execute(&pool)
        .await?;

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

    // Insert events in order
    ledger.append(event1.clone()).await?;
    ledger.append(event2.clone()).await?;
    ledger.append(event3.clone()).await?;

    // Verify the chain
    let report = verify_chain(&ledger).await?;
    assert!(report.valid);
    assert_eq!(report.events_checked, 3);
    assert_eq!(report.tail_hash, Some(event3.row_hash.clone()));

    Ok(())
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_verify_chain_tampered_hash_database()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (pool, ledger) = setup_test_ledger().await;

    // Clean up any existing data
    sqlx::query("DELETE FROM audit_ledger")
        .execute(&pool)
        .await?;

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

    // Insert events
    ledger.append(event1.clone()).await?;
    ledger.append(event2.clone()).await?;

    // Tamper with event1's row_hash in the database
    sqlx::query("UPDATE audit_ledger SET row_hash = 'tampered_hash' WHERE id = $1")
        .bind(id1)
        .execute(&pool)
        .await?;

    // Verify the chain - should detect tampering
    let report = verify_chain(&ledger).await?;
    assert!(!report.valid);
    assert_eq!(report.events_checked, 2);

    Ok(())
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_verify_chain_tampered_prev_hash_database()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (pool, ledger) = setup_test_ledger().await;

    // Clean up any existing data
    sqlx::query("DELETE FROM audit_ledger")
        .execute(&pool)
        .await?;

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

    // Insert events
    ledger.append(event1.clone()).await?;
    ledger.append(event2.clone()).await?;

    // Tamper with event2's prev_row_hash in the database
    sqlx::query("UPDATE audit_ledger SET prev_row_hash = 'wrong_prev_hash' WHERE id = $1")
        .bind(id2)
        .execute(&pool)
        .await?;

    // Verify the chain - should detect chain break
    let report = verify_chain(&ledger).await?;
    assert!(!report.valid);
    assert_eq!(report.events_checked, 2);

    Ok(())
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_verify_chain_single_event_database()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (pool, ledger) = setup_test_ledger().await;

    // Clean up any existing data
    sqlx::query("DELETE FROM audit_ledger")
        .execute(&pool)
        .await?;

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

    ledger.append(event1.clone()).await?;

    let report = verify_chain(&ledger).await?;
    assert!(report.valid);
    assert_eq!(report.events_checked, 1);
    assert_eq!(report.tail_hash, Some(event1.row_hash.clone()));

    Ok(())
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_verify_chain_empty_database() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let (pool, ledger) = setup_test_ledger().await;

    // Clean up any existing data
    sqlx::query("DELETE FROM audit_ledger")
        .execute(&pool)
        .await?;

    let report = verify_chain(&ledger).await?;
    assert!(report.valid);
    assert_eq!(report.events_checked, 0);
    assert_eq!(report.tail_hash, None);

    Ok(())
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_verify_chain_out_of_order_insertion()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (pool, ledger) = setup_test_ledger().await;

    // Clean up any existing data
    sqlx::query("DELETE FROM audit_ledger")
        .execute(&pool)
        .await?;

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

    // Insert events out of order (event2, event1, event3)
    ledger.append(event2.clone()).await?;
    ledger.append(event1.clone()).await?;
    ledger.append(event3.clone()).await?;

    // Verify the chain - should still be valid because verification orders by timestamp
    let report = verify_chain(&ledger).await?;
    assert!(report.valid);
    assert_eq!(report.events_checked, 3);
    assert_eq!(report.tail_hash, Some(event3.row_hash.clone()));

    Ok(())
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_audit_ledger_append_and_verify_integration()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (pool, ledger) = setup_test_ledger().await;

    // Clean up any existing data
    sqlx::query("DELETE FROM audit_ledger")
        .execute(&pool)
        .await?;

    let now = Utc::now();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    // Test appending through the ledger's append method (which computes hashes)
    let event1 = AuditEvent {
        id: id1,
        actor: "researcher-1".to_string(),
        resource_class: "genomic_variant".to_string(),
        action: "read".to_string(),
        cohort_scope: vec!["cohort-a".to_string()],
        timestamp: now,
        query_fingerprint: "query-1".to_string(),
        outcome: "success".to_string(),
        prev_row_hash: None,
        row_hash: String::new(), // Will be computed by append
    };

    let event2 = AuditEvent {
        id: id2,
        actor: "researcher-2".to_string(),
        resource_class: "transcriptomic_scrna".to_string(),
        action: "read".to_string(),
        cohort_scope: vec!["cohort-a".to_string()],
        timestamp: now,
        query_fingerprint: "query-2".to_string(),
        outcome: "success".to_string(),
        prev_row_hash: None,     // Will be set by append
        row_hash: String::new(), // Will be computed by append
    };

    ledger.append(event1).await?;
    ledger.append(event2).await?;

    // Verify the chain
    let report = verify_chain(&ledger).await?;
    assert!(report.valid);
    assert_eq!(report.events_checked, 2);

    // Verify the tail_hash matches what's in the database
    let tail = ledger.tail_hash().await?;
    assert_eq!(report.tail_hash, tail);

    Ok(())
}
