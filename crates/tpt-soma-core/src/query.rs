use sqlx::PgPool;
use crate::connection::Result;
use serde_json::Value;
use serde::Serialize;

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

pub async fn plex_match(
    pool: &PgPool,
    pattern: &str,
) -> Result<Vec<Value>> {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT row_to_json(t)::text::jsonb FROM plex_match($1) AS t"
    )
    .bind(pattern)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// Phase 1 query helpers

pub async fn get_variants_by_sample(
    pool: &PgPool,
    sample_id: &str,
) -> Result<Vec<VariantRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT v.variant_id, v.chromosome, v.position, v.reference, v.alternate, v.rsid, v.clinvar_id, sv.genotype
        FROM variants v
        JOIN sample_variants sv ON v.variant_id = sv.variant_id
        WHERE sv.sample_id = $1
        "#,
    )
    .bind(sample_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_expression_by_sample(
    pool: &PgPool,
    sample_id: &str,
) -> Result<Vec<ExpressionRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT sample_id, cell_id, gene_id, count
        FROM scrna_expression
        WHERE sample_id = $1
        "#,
    )
    .bind(sample_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_expression_by_gene(
    pool: &PgPool,
    gene_id: &str,
) -> Result<Vec<ExpressionRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT sample_id, cell_id, gene_id, count
        FROM scrna_expression
        WHERE gene_id = $1
        "#,
    )
    .bind(gene_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn join_variant_expression(
    pool: &PgPool,
    sample_id: &str,
    variant_id: &str,
    gene_id: &str,
) -> Result<Vec<VariantExpressionJoin>> {
    let rows = sqlx::query_as(
        r#"
        SELECT v.variant_id, v.chromosome, v.position, v.reference, v.alternate, v.rsid,
               e.cell_id, e.gene_id, e.count
        FROM variants v
        JOIN sample_variants sv ON v.variant_id = sv.variant_id
        JOIN scrna_expression e ON sv.sample_id = e.sample_id
        WHERE sv.sample_id = $1
          AND v.variant_id = $2
          AND e.gene_id = $3
        "#,
    )
    .bind(sample_id)
    .bind(variant_id)
    .bind(gene_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_cohort_samples(
    pool: &PgPool,
    cohort_id: &str,
) -> Result<Vec<SampleRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT s.sample_id, s.patient_id, s.source, s.dataset_provenance
        FROM samples s
        JOIN cohort_membership cm ON s.sample_id = cm.sample_id
        WHERE cm.cohort_id = $1
        "#,
    )
    .bind(cohort_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_umap_by_sample(
    pool: &PgPool,
    sample_id: &str,
) -> Result<Vec<UmapRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT sample_id, cell_id, umap1, umap2, cluster
        FROM scrna_umap
        WHERE sample_id = $1
        "#,
    )
    .bind(sample_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_umap_by_cluster(
    pool: &PgPool,
    sample_id: &str,
    cluster: &str,
) -> Result<Vec<UmapRecord>> {
    let rows = sqlx::query_as(
        r#"
        SELECT sample_id, cell_id, umap1, umap2, cluster
        FROM scrna_umap
        WHERE sample_id = $1 AND cluster = $2
        "#,
    )
    .bind(sample_id)
    .bind(cluster)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct VariantRecord {
    pub variant_id: String,
    pub chromosome: String,
    pub position: i32,
    pub reference: String,
    pub alternate: String,
    pub rsid: Option<String>,
    pub clinvar_id: Option<String>,
    pub genotype: Option<String>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ExpressionRecord {
    pub sample_id: uuid::Uuid,
    pub cell_id: String,
    pub gene_id: String,
    pub count: i32,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct VariantExpressionJoin {
    pub variant_id: String,
    pub chromosome: String,
    pub position: i32,
    pub reference: String,
    pub alternate: String,
    pub rsid: Option<String>,
    pub cell_id: String,
    pub gene_id: String,
    pub count: i32,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct SampleRecord {
    pub sample_id: uuid::Uuid,
    pub patient_id: Option<uuid::Uuid>,
    pub source: String,
    pub dataset_provenance: Option<String>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct UmapRecord {
    pub sample_id: uuid::Uuid,
    pub cell_id: String,
    pub umap1: f64,
    pub umap2: f64,
    pub cluster: String,
}