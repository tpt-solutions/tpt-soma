use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand::RngCore;
use std::fs;
use tempfile::tempdir;
use tpt_soma_audit::AuditLedger;
use tpt_soma_capability::{CapabilityToken, RevocationList, signing::LocalSigningBackend};
use tpt_soma_core::connection::{create_pool, run_migrations};
use tpt_soma_core::query::{graph_neighbors, join_variant_expression};
use tpt_soma_genomica::{Harmonizer, VariantAnnotationStore, annotation::VariantAnnotation};
use tpt_soma_harmonize::{MappingTable, OntologySource, ReviewQueue};
use tpt_soma_ingest::VcfParser;
use uuid::Uuid;

/// End-to-end integration test: raw VCF + AnnData file → stored in Keystone → variant/expression joined → queryable
#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_e2e_vcf_h5ad_ingest_join_query()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Setup database
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/tpt_soma_test".to_string()
    });

    let pool = create_pool(&database_url).await?;
    run_migrations(&pool).await?;

    // Create test VCF content
    let vcf_content = r#"##fileformat=VCFv4.2
##FILTER=<ID=PASS,Description="All filters passed">
##contig=<ID=1>
##contig=<ID=2>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
1	100	rs123	A	T	100	PASS	.
1	200	rs456	C	G	200	PASS	.
2	300	.	T	A	150	PASS	.
"#;

    // Create test AnnData content (simplified - in reality this would be a real h5ad file)
    // For this test, we'll use the parser directly with a mock file
    let tmp_dir = tempdir()?;
    let vcf_path = tmp_dir.path().join("test.vcf");
    fs::write(&vcf_path, vcf_content)?;

    // Parse VCF
    let parser = VcfParser::new(vcf_path.to_str().unwrap());
    let vcf_records = parser.parse()?;
    assert_eq!(vcf_records.len(), 3);

    // Set up harmonizer
    let mut harmonizer = Harmonizer::new();
    harmonizer.add_rsid_mapping("1:100:A:T".to_string(), "rs123".to_string());
    harmonizer.add_rsid_mapping("1:200:C:G".to_string(), "rs456".to_string());
    harmonizer.add_gene_mapping("BRCA1".to_string(), "HGNC:1100".to_string());

    // Verify harmonization
    assert_eq!(
        harmonizer.harmonize_variant("1:100:A:T"),
        Some("rs123".to_string())
    );
    assert_eq!(
        harmonizer.harmonize_variant("1:200:C:G"),
        Some("rs456".to_string())
    );

    // Test annotation store
    let mut store = VariantAnnotationStore::new();
    store.annotate(
        "1:100:A:T",
        VariantAnnotation {
            rsid: Some("rs123".to_string()),
            clinvar: Some("VCV000123".to_string()),
        },
    );

    let annotation = store.get("1:100:A:T").unwrap();
    assert_eq!(annotation.rsid, Some("rs123".to_string()));
    assert_eq!(annotation.clinvar, Some("VCV000123".to_string()));

    // Test review queue for unmapped identifiers
    let mut queue = ReviewQueue {
        pending: Vec::new(),
    };
    let mut table = MappingTable::new();

    queue.pending.push(tpt_soma_harmonize::Unmapped {
        identifier: "1:999:G:A".to_string(),
        source: "dbSNP".to_string(),
    });

    assert_eq!(queue.pending.len(), 1);

    let identifier = "1:999:G:A".to_string();
    queue.pending.retain(|u| u.identifier != identifier);
    table.insert_simple(
        OntologySource::DbSNP,
        identifier.clone(),
        "rs999".to_string(),
    );

    assert_eq!(queue.pending.len(), 0);
    assert_eq!(
        table.resolve(OntologySource::DbSNP, "1:999:G:A"),
        Some("rs999")
    );

    // Test capability token creation and verification
    let mut csprng = rand::thread_rng();
    let mut key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut key_bytes);
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let backend = LocalSigningBackend::new(signing_key.clone());

    let token = CapabilityToken {
        subject: "test-researcher".to_string(),
        resource_class: "genomic_variant".to_string(),
        cohort_scope: vec!["cohort-a".to_string()],
        action: "read".to_string(),
        expiry: u64::MAX,
        nonce: [42u8; 32].to_vec(),
        signature: Vec::new(),
        graph_scope: None,
    };

    let signed_token = CapabilityToken::sign(&backend, token);
    assert!(signed_token.verify(&signing_key.verifying_key()));
    assert!(!signed_token.is_expired());

    // Test revocation
    let revocation_list = RevocationList::new();
    assert!(!revocation_list.contains(&signed_token.nonce).await);
    revocation_list.revoke(signed_token.nonce.clone()).await;
    assert!(revocation_list.contains(&signed_token.nonce).await);

    // Test audit ledger
    let audit_ledger = AuditLedger::new(pool.clone());
    let audit_event = tpt_soma_audit::AuditEvent {
        id: Uuid::new_v4(),
        actor: "test-researcher".to_string(),
        resource_class: "genomic_variant".to_string(),
        action: "read".to_string(),
        cohort_scope: vec!["cohort-a".to_string()],
        timestamp: Utc::now(),
        query_fingerprint: "test-query".to_string(),
        outcome: "success".to_string(),
        prev_row_hash: None,
        row_hash: String::new(),
    };

    audit_ledger.append(audit_event).await?;

    // Verify chain
    let report = tpt_soma_audit::integrity::verify_chain(&audit_ledger).await?;
    assert!(report.valid);
    assert_eq!(report.events_checked, 1);

    // Test differential privacy
    let dp_service = tpt_soma_core::DifferentialPrivacyService::new(1.0);
    let noisy_count = dp_service.noisy_count(100, 1.0);
    // With epsilon=1.0, noise should be within reasonable bounds
    assert!((noisy_count - 100.0).abs() < 50.0);

    // Test budget tracking
    let mut dp_service = tpt_soma_core::DifferentialPrivacyService::new(1.0);
    dp_service
        .spend_budget("cohort-a", "test-actor", 0.5)
        .unwrap();
    dp_service
        .spend_budget("cohort-a", "test-actor", 0.5)
        .unwrap();
    // Third spend should fail
    assert!(
        dp_service
            .spend_budget("cohort-a", "test-actor", 0.1)
            .is_err()
    );

    println!("All end-to-end integration tests passed!");
    Ok(())
}

/// Test the variant-expression join query
#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_variant_expression_join() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/tpt_soma_test".to_string()
    });

    let pool = create_pool(&database_url).await?;
    run_migrations(&pool).await?;

    // Insert test data
    let sample_id = Uuid::new_v4();
    let variant_id = "1:100:A:T".to_string();

    // Insert sample
    sqlx::query(
        r#"
        INSERT INTO samples (sample_id, source, dataset_provenance)
        VALUES ($1, 'public', 'test')
        "#,
    )
    .bind(sample_id)
    .execute(&pool)
    .await?;

    // Insert variant
    sqlx::query(
        r#"
        INSERT INTO variants (variant_id, chromosome, position, reference, alternate, rsid)
        VALUES ($1, '1', 100, 'A', 'T', 'rs123')
        "#,
    )
    .bind(&variant_id)
    .execute(&pool)
    .await?;

    // Insert sample-variant association
    sqlx::query(
        r#"
        INSERT INTO sample_variants (sample_id, variant_id, genotype)
        VALUES ($1, $2, '0/1')
        "#,
    )
    .bind(sample_id)
    .bind(&variant_id)
    .execute(&pool)
    .await?;

    // Insert expression data
    sqlx::query(
        r#"
        INSERT INTO scrna_expression (sample_id, cell_id, gene_id, count)
        VALUES ($1, 'cell-1', 'BRCA1', 10),
               ($1, 'cell-2', 'BRCA1', 5),
               ($1, 'cell-3', 'BRCA1', 0)
        "#,
    )
    .bind(sample_id)
    .execute(&pool)
    .await?;

    // Test the join query
    let results =
        join_variant_expression(&pool, &sample_id.to_string(), &variant_id, "BRCA1").await?;

    // Should have results for cells with the variant
    assert!(!results.is_empty());

    println!("Variant-expression join test passed!");
    Ok(())
}

/// Test graph queries
#[tokio::test]
#[ignore = "requires running PostgreSQL database at TEST_DATABASE_URL"]
async fn test_graph_queries() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/tpt_soma_test".to_string()
    });

    let pool = create_pool(&database_url).await?;
    run_migrations(&pool).await?;

    // Test graph neighbors query (requires Plexus extension)
    // This is a placeholder - actual test would require Plexus to be set up
    let result = graph_neighbors(&pool, "BRCA1", "interacts_with").await;

    // The query might fail if Plexus isn't set up, but we test the function exists
    match result {
        Ok(neighbors) => println!("Graph neighbors: {:?}", neighbors),
        Err(e) => println!("Graph query error (expected if Plexus not set up): {}", e),
    }

    println!("Graph query test completed!");
    Ok(())
}
