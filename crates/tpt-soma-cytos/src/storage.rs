use csv::ReaderBuilder;
use sqlx::PgPool;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use tpt_soma_core::connection::Result;
use tpt_soma_ingest::h5ad::ScRNASeqRecord;

/// Parse a Scanpy cluster-labels CSV (`cell_id,cluster`) into a map. Rows that
/// fail to decode are skipped rather than aborting the whole ingest.
pub fn build_cluster_map(labels: impl Read) -> HashMap<String, String> {
    let mut labels_reader = ReaderBuilder::new().has_headers(true).from_reader(labels);
    let mut cluster_map = HashMap::new();
    for record in labels_reader.records().flatten() {
        let cell_id = record.get(0).unwrap_or("").to_string();
        let cluster = record.get(1).unwrap_or("").to_string();
        cluster_map.insert(cell_id, cluster);
    }
    cluster_map
}

pub async fn ingest_scanpy_output(
    pool: &PgPool,
    sample_id: &str,
    umap_path: &str,
    labels_path: &str,
) -> Result<()> {
    // Read UMAP coordinates
    let umap_file = File::open(umap_path)
        .map_err(|e| tpt_soma_core::connection::CoreError::Pool(e.to_string()))?;
    let mut umap_reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(BufReader::new(umap_file));

    // Build cluster map from cluster labels
    let labels_file = File::open(labels_path)
        .map_err(|e| tpt_soma_core::connection::CoreError::Pool(e.to_string()))?;
    let cluster_map = build_cluster_map(BufReader::new(labels_file));

    // Insert UMAP coordinates with cluster info
    for result in umap_reader.records() {
        let record =
            result.map_err(|e| tpt_soma_core::connection::CoreError::Pool(e.to_string()))?;
        let cell_id = record.get(0).unwrap_or("").to_string();
        let umap1: f64 = record.get(1).unwrap_or("0").parse().unwrap_or(0.0);
        let umap2: f64 = record.get(2).unwrap_or("0").parse().unwrap_or(0.0);
        let cluster = cluster_map
            .get(&cell_id)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        sqlx::query(
            r#"
            INSERT INTO scrna_umap (sample_id, cell_id, umap1, umap2, cluster)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (sample_id, cell_id) DO UPDATE SET
                umap1 = EXCLUDED.umap1,
                umap2 = EXCLUDED.umap2,
                cluster = EXCLUDED.cluster
            "#,
        )
        .bind(sample_id)
        .bind(&cell_id)
        .bind(umap1)
        .bind(umap2)
        .bind(&cluster)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn ingest_expression_matrix(
    pool: &PgPool,
    sample_id: &str,
    records: &[ScRNASeqRecord],
) -> Result<()> {
    for record in records {
        sqlx::query(
            r#"
            INSERT INTO scrna_expression (sample_id, cell_id, gene_id, count)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (sample_id, cell_id, gene_id) DO UPDATE SET
                count = EXCLUDED.count
            "#,
        )
        .bind(sample_id)
        .bind(&record.cell_id)
        .bind(&record.gene_id)
        .bind(record.count as i32)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn build_cluster_map_parses_labels() {
        let csv = "cell_id,cluster\ncell-1,0\ncell-2,1\ncell-3,0\n";
        let map = build_cluster_map(Cursor::new(csv));
        assert_eq!(map.get("cell-1").map(String::as_str), Some("0"));
        assert_eq!(map.get("cell-2").map(String::as_str), Some("1"));
        assert_eq!(map.get("cell-3").map(String::as_str), Some("0"));
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn build_cluster_map_handles_missing_cluster_column() {
        let csv = "cell_id,cluster\ncell-1,\ncell-2,3\n";
        let map = build_cluster_map(Cursor::new(csv));
        assert_eq!(map.get("cell-1").map(String::as_str), Some(""));
        assert_eq!(map.get("cell-2").map(String::as_str), Some("3"));
    }

    #[test]
    fn build_cluster_map_skips_ragged_rows() {
        let csv = "cell_id,cluster\ncell-1\ncell-2,3\n";
        let map = build_cluster_map(Cursor::new(csv));
        assert!(!map.contains_key("cell-1"));
        assert_eq!(map.get("cell-2").map(String::as_str), Some("3"));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn build_cluster_map_empty_input() {
        let map = build_cluster_map(Cursor::new("cell_id,cluster\n"));
        assert!(map.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_ingest_scanpy_output() {
        // Would require a test database
    }
}
