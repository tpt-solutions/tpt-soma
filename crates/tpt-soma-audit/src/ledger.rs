use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Advisory-lock key that serialises chain appends across every API replica.
/// Any fixed value works; it only has to be the same everywhere.
const CHAIN_LOCK_KEY: i64 = 0x7470_7473_6175_6431;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEvent {
    pub id: Uuid,
    pub actor: String,
    pub resource_class: String,
    pub action: String,
    pub cohort_scope: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub query_fingerprint: String,
    pub outcome: String,
    pub prev_row_hash: Option<String>,
    pub row_hash: String,
}

pub struct AuditLedger {
    pub pool: PgPool,
}

impl AuditLedger {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Appends one event to the hash chain.
    ///
    /// The tail read and the insert run in one transaction under an advisory
    /// lock. Without the lock, two concurrent appends can link to the same
    /// predecessor and fork the chain, which verification then reports as
    /// tampering.
    pub async fn append(&self, mut event: AuditEvent) -> Result<(), AuditError> {
        // Postgres stores TIMESTAMPTZ at microsecond precision. The hash must be
        // computed over the value the database returns, or a recompute from
        // stored rows never matches.
        event.timestamp = truncate_to_micros(event.timestamp);

        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(CHAIN_LOCK_KEY)
            .execute(&mut *tx)
            .await?;

        let prev_hash = sqlx::query_scalar::<_, Option<String>>(
            "SELECT row_hash FROM audit_ledger ORDER BY seq DESC LIMIT 1",
        )
        .fetch_optional(&mut *tx)
        .await?
        .flatten();
        event.prev_row_hash = prev_hash.clone();

        event.row_hash = compute_row_hash(prev_hash.as_deref(), &event)?;

        sqlx::query(
            r#"
            INSERT INTO audit_ledger
                (id, actor, resource_class, action, cohort_scope, timestamp, query_fingerprint, outcome, prev_row_hash, row_hash)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(event.id)
        .bind(event.actor)
        .bind(event.resource_class)
        .bind(event.action)
        .bind(&event.cohort_scope)
        .bind(event.timestamp)
        .bind(event.query_fingerprint)
        .bind(event.outcome)
        .bind(event.prev_row_hash)
        .bind(event.row_hash)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn tail_hash(&self) -> Result<Option<String>, AuditError> {
        let row = sqlx::query_scalar::<_, Option<String>>(
            "SELECT row_hash FROM audit_ledger ORDER BY seq DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.flatten())
    }

    pub async fn verify_chain(
        &self,
    ) -> Result<crate::integrity::ChainReport, crate::integrity::IntegrityError> {
        crate::integrity::verify_chain(self).await
    }
}

/// `H(prev_row_hash || canonical event payload)`. The writer and the verifier
/// both call this, so the two cannot drift apart.
pub fn compute_row_hash(
    prev_hash: Option<&str>,
    event: &AuditEvent,
) -> Result<String, serde_json::Error> {
    let mut hasher = Sha256::new();
    if let Some(prev) = prev_hash {
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
    hasher.update(serde_json::to_vec(&payload)?);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Truncates to the precision TIMESTAMPTZ stores.
pub fn truncate_to_micros(ts: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::from_timestamp_micros(ts.timestamp_micros()).unwrap_or(ts)
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
