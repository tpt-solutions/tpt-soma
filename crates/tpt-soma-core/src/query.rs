use sqlx::PgPool;
use crate::connection::Result;

pub async fn graph_neighbors(
    pool: &PgPool,
    node_id: &str,
    edge_label: &str,
) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT target_id FROM graph_neighbors($1, $2)"
    )
    .bind(node_id)
    .bind(edge_label)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn graph_bfs(
    pool: &PgPool,
    start_id: &str,
    max_depth: i32,
) -> Result<Vec<(String, i32)>> {
    let rows = sqlx::query_as(
        "SELECT node_id, depth FROM graph_bfs($1, $2)"
    )
    .bind(start_id)
    .bind(max_depth)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
