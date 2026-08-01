use tpt_soma_core::connection::Result;
use sqlx::PgPool;

pub async fn ingest_scanpy_output(pool: &PgPool, sample_id: &str, umap_path: &str, labels_path: &str) -> Result<()> {
    let _ = (pool, sample_id, umap_path, labels_path);
    Ok(())
}
