use tpt_soma_ingest::h5ad::AnnDataParser;

const DENSE_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testdata/mini_scrna_dense.h5ad"
);
const SPARSE_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testdata/mini_scrna_sparse.h5ad"
);

#[test]
fn test_h5ad_dense_fixture() {
    let result = AnnDataParser::new(DENSE_FIXTURE).parse().unwrap();

    assert_eq!(result.n_cells(), 3);
    assert_eq!(result.n_genes(), 4);
    assert_eq!(result.n_records(), 7);

    assert_eq!(result.metadata.cell_ids, vec!["cell-1", "cell-2", "cell-3"]);
    assert_eq!(
        result.metadata.gene_ids,
        vec!["BRCA1", "TP53", "GAPDH", "ACTB"]
    );

    let cell1: Vec<_> = result
        .records
        .iter()
        .filter(|r| r.cell_id == "cell-1")
        .collect();
    assert_eq!(cell1.len(), 2);
    let brca1 = cell1.iter().find(|r| r.gene_id == "BRCA1").unwrap();
    assert_eq!(brca1.count, 10);
    let gapdh = cell1.iter().find(|r| r.gene_id == "GAPDH").unwrap();
    assert_eq!(gapdh.count, 200);

    // Zero entries must be dropped.
    assert!(
        !result
            .records
            .iter()
            .any(|r| r.cell_id == "cell-3" && r.gene_id == "BRCA1")
    );
    let genes: std::collections::HashSet<&str> =
        result.records.iter().map(|r| r.gene_id.as_str()).collect();
    assert_eq!(
        genes,
        ["BRCA1", "TP53", "GAPDH", "ACTB"].into_iter().collect()
    );
}

#[test]
fn test_h5ad_sparse_fixture() {
    let result = AnnDataParser::new(SPARSE_FIXTURE).parse().unwrap();

    assert_eq!(result.n_cells(), 4);
    assert_eq!(result.n_genes(), 5);
    assert_eq!(result.n_records(), 7);

    // CSR layout: indptr [0,2,4,5,7], indices [0,2,0,1,4,3,0]
    let cell2: Vec<_> = result
        .records
        .iter()
        .filter(|r| r.cell_id == "cell-2")
        .collect();
    assert_eq!(cell2.len(), 2);
    assert!(cell2.iter().any(|r| r.gene_id == "BRCA1"));
    assert!(cell2.iter().any(|r| r.gene_id == "TP53"));

    let cell3 = result
        .records
        .iter()
        .find(|r| r.cell_id == "cell-3")
        .unwrap();
    assert_eq!(cell3.gene_id, "MYC");
    assert_eq!(cell3.count, 1);

    assert!(
        result
            .records
            .iter()
            .all(|r| r.sample_id.is_empty() && r.count == 1)
    );
}
