use sqlx::PgPool;
use crate::connection::Result;

pub async fn list_applied(pool: &PgPool) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT version FROM _sqlx_migrations ORDER BY version"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
