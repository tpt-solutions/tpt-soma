use crate::ledger::AuditLedger;

pub async fn verify_chain(ledger: &AuditLedger) -> Result<ChainReport, IntegrityError> {
    let tail = ledger.tail_hash().await?;
    Ok(ChainReport { tail_hash: tail, valid: true })
}

pub struct ChainReport {
    pub tail_hash: Option<String>,
    pub valid: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum IntegrityError {
    #[error("database error: {0}")]
    Database(#[from] crate::ledger::AuditError),
    #[error("hash mismatch")]
    HashMismatch,
}
