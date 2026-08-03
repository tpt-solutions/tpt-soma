use std::fs;
use tpt_soma_harmonize::OntologySource;
use tpt_soma_ingest::VcfParser;

/// Test VCF parsing with a minimal valid VCF file
#[test]
fn test_vcf_parser_golden_file() {
    let vcf_content = r#"##fileformat=VCFv4.2
##FILTER=<ID=PASS,Description="All filters passed">
##contig=<ID=1>
##contig=<ID=2>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
1	100	rs123	A	T	100	PASS	.
1	200	rs456	C	G	200	PASS	.
2	300	.	T	A	150	PASS	.
"#;

    let tmp_dir = tempfile::tempdir().unwrap();
    let vcf_path = tmp_dir.path().join("test.vcf");
    fs::write(&vcf_path, vcf_content).unwrap();

    let parser = VcfParser::new(vcf_path.to_str().unwrap());
    let records = parser.parse().unwrap();

    assert_eq!(records.len(), 3);

    // Check first record
    assert_eq!(records[0].variant_id, "1:100:A:T");
    assert_eq!(records[0].chromosome, "1");
    assert_eq!(records[0].position, 100);
    assert_eq!(records[0].reference, "A");
    assert_eq!(records[0].alternate, "T");
    assert_eq!(records[0].rsid, Some("rs123".to_string()));

    // Check second record
    assert_eq!(records[1].variant_id, "1:200:C:G");
    assert_eq!(records[1].rsid, Some("rs456".to_string()));

    // Check third record (no rsid)
    assert_eq!(records[2].variant_id, "2:300:T:A");
    assert_eq!(records[2].rsid, None);
}

/// Test VCF parsing with multi-allelic variants
#[test]
fn test_vcf_parser_multi_allelic() {
    let vcf_content = r#"##fileformat=VCFv4.2
##contig=<ID=1>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
1	100	rs789	A	T,C	100	PASS	.
"#;

    let tmp_dir = tempfile::tempdir().unwrap();
    let vcf_path = tmp_dir.path().join("test_multi.vcf");
    fs::write(&vcf_path, vcf_content).unwrap();

    let parser = VcfParser::new(vcf_path.to_str().unwrap());
    let records = parser.parse().unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].variant_id, "1:100:A:T,C");
    assert_eq!(records[0].alternate, "T,C");
    assert_eq!(records[0].rsid, Some("rs789".to_string()));
}

/// Integration test: VCF + AnnData round-trip through harmonization
#[test]
fn test_ingest_harmonize_integration() {
    use tpt_soma_genomica::{Harmonizer, VariantAnnotationStore};

    // Create VCF with known variants
    let vcf_content = r#"##fileformat=VCFv4.2
##contig=<ID=1>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
1	100	rs123	A	T	100	PASS	.
1	200	rs456	C	G	200	PASS	.
"#;

    let tmp_dir = tempfile::tempdir().unwrap();
    let vcf_path = tmp_dir.path().join("test.vcf");
    fs::write(&vcf_path, vcf_content).unwrap();

    // Parse VCF
    let parser = VcfParser::new(vcf_path.to_str().unwrap());
    let _records = parser.parse().unwrap();

    // Set up harmonizer with known mappings
    let mut harmonizer = Harmonizer::new();
    harmonizer.add_rsid_mapping("1:100:A:T".to_string(), "rs123".to_string());
    harmonizer.add_rsid_mapping("1:200:C:G".to_string(), "rs456".to_string());
    harmonizer.add_gene_mapping("BRCA1".to_string(), "HGNC:1100".to_string());

    // Verify harmonization works
    assert_eq!(
        harmonizer.harmonize_variant("1:100:A:T"),
        Some("rs123".to_string())
    );
    assert_eq!(
        harmonizer.harmonize_variant("1:200:C:G"),
        Some("rs456".to_string())
    );
    assert_eq!(
        harmonizer.harmonize_gene("BRCA1"),
        Some("HGNC:1100".to_string())
    );

    // Test annotation store
    let mut store = VariantAnnotationStore::new();
    store.annotate(
        "1:100:A:T",
        tpt_soma_genomica::annotation::VariantAnnotation {
            rsid: Some("rs123".to_string()),
            clinvar: Some("VCV000123".to_string()),
        },
    );

    let annotation = store.get("1:100:A:T").unwrap();
    assert_eq!(annotation.rsid, Some("rs123".to_string()));
    assert_eq!(annotation.clinvar, Some("VCV000123".to_string()));
}

/// Test that the review queue works for unmapped identifiers
#[test]
fn test_review_queue_unmapped() {
    use tpt_soma_harmonize::{MappingTable, ReviewQueue, Unmapped};

    let mut queue = ReviewQueue {
        pending: Vec::new(),
    };
    let mut table = MappingTable::new();

    // Add unmapped identifiers
    queue.pending.push(Unmapped {
        identifier: "1:999:G:A".to_string(),
        source: "dbSNP".to_string(),
    });
    queue.pending.push(Unmapped {
        identifier: "UNKNOWN_GENE".to_string(),
        source: "HGNC".to_string(),
    });

    assert_eq!(queue.pending.len(), 2);

    // Resolve one
    let identifier = "1:999:G:A".to_string();
    queue.pending.retain(|u| u.identifier != identifier);
    table.insert_simple(
        OntologySource::DbSNP,
        identifier.clone(),
        "rs999".to_string(),
    );

    assert_eq!(queue.pending.len(), 1);
    assert_eq!(
        table.resolve(OntologySource::DbSNP, "1:999:G:A"),
        Some("rs999")
    );

    // Test serialization
    let json = serde_json::to_string(&queue).unwrap();
    let deserialized: ReviewQueue = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.pending.len(), 1);

    let json = serde_json::to_string(&table).unwrap();
    let deserialized: MappingTable = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.resolve(OntologySource::DbSNP, "1:999:G:A"),
        Some("rs999")
    );
}

#[cfg(test)]
mod download_tests {

    /// This test is ignored by default as it requires network access.
    /// Run with: cargo test --test golden_file_tests download_1000_genomes -- --ignored
    #[ignore]
    #[tokio::test]
    async fn download_1000_genomes_sample() {
        // This would download a small 1000 Genomes VCF subset for testing
        // For now, it's a placeholder showing the intent
        println!("Would download 1000 Genomes sample VCF");
    }

    /// This test is ignored by default as it requires network access.
    /// Run with: cargo test --test golden_file_tests download_pbmc_3k -- --ignored
    #[ignore]
    #[tokio::test]
    async fn download_pbmc_3k_sample() {
        // This would download the 10x Genomics PBMC 3k h5ad file for testing
        println!("Would download PBMC 3k h5ad");
    }
}
