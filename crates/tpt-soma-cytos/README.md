# tpt-soma-cytos

Single-cell RNA-seq (10x/AnnData) Scanpy orchestration and storage.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-cytos` runs single-cell transcriptomics analysis through Scanpy and stores the results in Postgres. It generates the preprocessing script, runs it, and ingests the expression matrix, cluster assignments and UMAP coordinates that come back. Downstream queries then read expression and clustering results by sample, gene or cluster.

## Features

- **Script generation** (`ScanpyScriptGenerator`, `generate_preprocessing_script`): writes a Scanpy preprocessing script for an AnnData input (cell and gene filtering, total-count normalization, log transform, highly variable genes, scaling, PCA, neighbors, UMAP and Leiden clustering).
- **Orchestration** (`ScanpyOrchestrator`, `ScanpyResult`): runs the generated script in a Python environment and collects its outputs. Failures are reported as `ScanpyError`.
- **Cluster mapping** (`build_cluster_map`): turns the per-cell cluster labels into a map from cluster to cell set.
- **Ingestion** (`ingest_scanpy_output`, `ingest_expression_matrix`): loads the expression matrix and the clustering and UMAP results into Postgres.
- **Storage** (`storage`): the Postgres layer for expression, samples and UMAP records. Read paths are in [`tpt-soma-core`](../tpt-soma-core/README.md) (`get_expression_by_gene`, `get_umap_by_cluster`).

## Requirements

Orchestration needs a Python environment with `scanpy` installed and `python` on `PATH`. The Rust code does not link to Python. `ScanpyOrchestrator` runs the generated script as a subprocess.

## Data flow

1. A 10x or AnnData file is ingested by [`tpt-soma-ingest`](../tpt-soma-ingest/README.md), which records its metadata and checksum.
2. `ScanpyScriptGenerator` produces the preprocessing script for that file.
3. `ScanpyOrchestrator` runs it and returns the outputs.
4. `ingest_scanpy_output` stores expression, clusters and UMAP coordinates.
5. Queries are served through the `expression` and `umap` routes in [`tpt-soma-api`](../tpt-soma-api/README.md), and access is audited.

## Crate layout

| Module | Responsibility |
|---|---|
| `scanpy` | `ScanpyOrchestrator`, `ScanpyScriptGenerator`, `ScanpyResult`, `ScanpyError`, script generation, cluster mapping |
| `storage` | Persistence of expression, sample and UMAP data |

## Running the tests

```sh
cargo test -p tpt-soma-cytos
```

The unit tests cover script generation and cluster-map parsing without Python. Orchestration tests need Python with Scanpy installed. The coverage floor is 60% (see [coverage targets](../../docs/testing/coverage-targets.md)).

## Related crates

- [`tpt-soma-ingest`](../tpt-soma-ingest/README.md): reads the AnnData files this crate analyzes
- [`tpt-soma-genomica`](../tpt-soma-genomica/README.md): variant data that is joined to expression
- [`tpt-soma-core`](../tpt-soma-core/README.md): the Postgres pool and query helpers

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
