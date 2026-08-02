use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub struct ComplianceReporter<'a> {
    pool: &'a PgPool,
}

impl<'a> ComplianceReporter<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn access_to_cohort(
        &self,
        cohort_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<ComplianceEntry>, ComplianceError> {
        let rows = sqlx::query_as(
            r#"
            SELECT actor, action, timestamp, outcome FROM audit_ledger
            WHERE $1 = ANY(cohort_scope)
              AND timestamp >= $2
              AND timestamp <= $3
            ORDER BY timestamp ASC
            "#,
        )
        .bind(cohort_id)
        .bind(from)
        .bind(to)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct ComplianceEntry {
    pub actor: String,
    pub action: String,
    pub timestamp: DateTime<Utc>,
    pub outcome: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ComplianceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
