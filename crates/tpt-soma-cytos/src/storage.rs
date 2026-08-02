use csv::ReaderBuilder;
use sqlx::PgPool;
use std::fs::File;
use std::io::BufReader;
use tpt_soma_core::connection::Result;
use tpt_soma_ingest::h5ad::ScRNASeqRecord;

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

    // Read cluster labels
    let labels_file = File::open(labels_path)
        .map_err(|e| tpt_soma_core::connection::CoreError::Pool(e.to_string()))?;
    let mut labels_reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(BufReader::new(labels_file));

    // Build cluster map
    let mut cluster_map = std::collections::HashMap::new();
    for result in labels_reader.records() {
        let record =
            result.map_err(|e| tpt_soma_core::connection::CoreError::Pool(e.to_string()))?;
        let cell_id = record.get(0).unwrap_or("").to_string();
        let cluster = record.get(1).unwrap_or("").to_string();
        cluster_map.insert(cell_id, cluster);
    }

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
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_umap() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "cell_id,UMAP1,UMAP2").unwrap();
        writeln!(file, "cell_1,1.0,2.0").unwrap();
        writeln!(file, "cell_2,3.0,4.0").unwrap();
        file.flush().unwrap();
        file
    }

    fn create_test_labels() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "cell_id,leiden").unwrap();
        writeln!(file, "cell_1,0").unwrap();
        writeln!(file, "cell_2,1").unwrap();
        file.flush().unwrap();
        file
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_ingest_scanpy_output() {
        // Would require a test database
    }
}
