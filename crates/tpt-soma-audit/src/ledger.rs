use sqlx::PgPool;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pool: PgPool,
}

impl AuditLedger {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn tail_hash(&self) -> Result<Option<String>, AuditError> {
        let row = sqlx::query_scalar::<_, Option<String>>(
            "SELECT row_hash FROM audit_ledger ORDER BY timestamp DESC LIMIT 1"
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.flatten())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
