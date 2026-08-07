// Keystone load test at realistic Phase 1 scale.
//
// Generates a sparse scRNA-seq matrix (samples x cells x genes) plus variant
// calls, bulk-inserts it into Keystone, and times representative queries.
//
// Scale is configurable via env vars (defaults target a moderate run so the
// test is usable as a perf smoke gate):
//   TPT_LOAD_SAMPLES  (default 3)
//   TPT_LOAD_CELLS    (default 2000)
//   TPT_LOAD_GENES    (default 2000)
//   TPT_LOAD_DENSITY  (default 0.15  -> fraction of nonzero counts)
//
// Run with:
//   cargo test -p tpt-soma-core --test load_test -- --ignored --nocapture

use rand::Rng;
use std::time::Instant;
use tpt_soma_core::query::{get_expression_by_sample, get_variants_by_sample};
use tpt_soma_core::test_helpers::test_pool;

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_sparse_scrna_load_at_scale() {
    let samples = env_usize("TPT_LOAD_SAMPLES", 3);
    let cells = env_usize("TPT_LOAD_CELLS", 2000);
    let genes = env_usize("TPT_LOAD_GENES", 2000);
    let density = env_f64("TPT_LOAD_DENSITY", 0.15);

    let n_nonzero = (samples * cells * genes) as f64 * density;
    println!(
        "scale: {} samples x {} cells x {} genes @ density {} (~{} sparse rows)",
        samples, cells, genes, density, n_nonzero as usize
    );

    let pool = test_pool().await.unwrap();

    // Clean slate for the load-test namespace.
    sqlx::query(
        "DELETE FROM sample_variants WHERE sample_id IN (
            SELECT sample_id FROM samples WHERE dataset_provenance = 'load-test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "DELETE FROM scrna_expression WHERE sample_id IN (
            SELECT sample_id FROM samples WHERE dataset_provenance = 'load-test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM samples WHERE dataset_provenance = 'load-test'")
        .execute(&pool)
        .await
        .unwrap();

    // 1) Insert samples (UUID ids).
    let sample_ids: Vec<String> = (0..samples)
        .map(|i| uuid::Uuid::from_u128(i as u128 + 1).to_string())
        .collect();
    let mut insert_samples = String::from(
        "INSERT INTO samples (sample_id, patient_id, source, dataset_provenance) VALUES ",
    );
    let mut binds: Vec<String> = Vec::new();
    for id in &sample_ids {
        binds.push(format!("('{}', NULL, 'public', 'load-test')", id));
    }
    insert_samples.push_str(&binds.join(", "));
    sqlx::query(&insert_samples).execute(&pool).await.unwrap();

    // 2) Insert genes as variants (variant_id doubles as gene_id for the test).
    let mut rng = rand::thread_rng();
    let mut variant_rows: Vec<String> = Vec::with_capacity(genes);
    for g in 0..genes {
        variant_rows.push(format!("('g{}', '1', {}, 'A', 'T', NULL)", g, g as i32 + 1));
    }
    let bulk_variants = format!(
        "INSERT INTO variants (variant_id, chromosome, position, reference, alternate, rsid) VALUES {} ON CONFLICT DO NOTHING",
        variant_rows.join(", ")
    );
    sqlx::query(&bulk_variants).execute(&pool).await.unwrap();

    // 3) Bulk insert sample-variant links (every sample has ~1% of genes).
    let t0 = Instant::now();
    let mut sv_binds: Vec<String> = Vec::new();
    for id in &sample_ids {
        for g in 0..genes {
            if rng.gen_bool(0.01) {
                sv_binds.push(format!("('{}', 'g{}', '0/1')", id, g));
            }
        }
    }
    let bulk_sv = format!(
        "INSERT INTO sample_variants (sample_id, variant_id, genotype) VALUES {}",
        sv_binds.join(", ")
    );
    sqlx::query(&bulk_sv).execute(&pool).await.unwrap();
    println!(
        "inserted {} sample_variant links in {:.2}s",
        sv_binds.len(),
        t0.elapsed().as_secs_f64()
    );

    // 4) Bulk insert sparse expression via UNNEST (much faster than row-by-row).
    let t1 = Instant::now();
    let mut n_rows = 0usize;
    for id in &sample_ids {
        let mut cell_ids: Vec<String> = Vec::new();
        let mut gene_ids: Vec<String> = Vec::new();
        let mut counts: Vec<i32> = Vec::new();
        for c in 0..cells {
            for g in 0..genes {
                if rng.gen_bool(density) {
                    cell_ids.push(format!("cell-{:05}", c));
                    gene_ids.push(format!("g{}", g));
                    counts.push(rng.gen_range(1..=50));
                }
            }
        }
        n_rows += cell_ids.len();
        sqlx::query(
            "INSERT INTO scrna_expression (sample_id, cell_id, gene_id, count) \
             SELECT $1, unnest($2::text[]), unnest($3::text[]), unnest($4::int[])",
        )
        .bind(id)
        .bind(&cell_ids)
        .bind(&gene_ids)
        .bind(&counts)
        .execute(&pool)
        .await
        .unwrap();
    }
    println!(
        "inserted {} sparse expression rows in {:.2}s ({:.0} rows/s)",
        n_rows,
        t1.elapsed().as_secs_f64(),
        n_rows as f64 / t1.elapsed().as_secs_f64()
    );

    // 5) Time representative queries.
    let t2 = Instant::now();
    let mut total_rows = 0usize;
    for id in &sample_ids {
        let rows = get_variants_by_sample(&pool, id).await.unwrap();
        total_rows += rows.len();
    }
    let variants_q = t2.elapsed();
    println!(
        "variants-by-sample x{} queries: {:.2}s ({:.3}s/query, {} rows)",
        samples,
        variants_q.as_secs_f64(),
        variants_q.as_secs_f64() / samples as f64,
        total_rows
    );

    let t3 = Instant::now();
    let mut total_expr = 0usize;
    for id in &sample_ids {
        let rows = get_expression_by_sample(&pool, id).await.unwrap();
        total_expr += rows.len();
    }
    let expr_q = t3.elapsed();
    println!(
        "expression-by-sample x{} queries: {:.2}s ({:.3}s/query, {} rows)",
        samples,
        expr_q.as_secs_f64(),
        expr_q.as_secs_f64() / samples as f64,
        total_expr
    );

    // 6) Soft perf gates (very generous; tighten once baseline is measured).
    assert!(
        expr_q.as_secs_f64() / (samples as f64) < 10.0,
        "expression query too slow"
    );
    assert!(
        variants_q.as_secs_f64() / (samples as f64) < 10.0,
        "variants query too slow"
    );
    assert!(total_expr > 0);

    // 7) Cleanup.
    sqlx::query(
        "DELETE FROM sample_variants WHERE sample_id IN (
            SELECT sample_id FROM samples WHERE dataset_provenance = 'load-test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "DELETE FROM scrna_expression WHERE sample_id IN (
            SELECT sample_id FROM samples WHERE dataset_provenance = 'load-test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM samples WHERE dataset_provenance = 'load-test'")
        .execute(&pool)
        .await
        .unwrap();

    println!("load test complete");
}
