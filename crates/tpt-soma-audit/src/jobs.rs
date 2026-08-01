use sqlx::PgPool;
use crate::{ledger::AuditLedger, compliance::ComplianceReporter, AuditError};

pub async fn run_integrity_check(pool: &PgPool) -> Result<crate::integrity::ChainReport, crate::IntegrityError> {
    let ledger = AuditLedger::new(pool.clone());
    let report = ledger.verify_chain().await?;
    if !report.valid {
        return Err(crate::IntegrityError::HashMismatch);
    }
    Ok(report)
}

pub async fn generate_compliance_report(
    pool: &PgPool,
    cohort: &str,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<crate::compliance::ComplianceEntry>, AuditError> {
    let reporter = ComplianceReporter::new(pool);
    let entries = reporter.access_to_cohort(cohort, from, to).await?;
    Ok(entries)
}
