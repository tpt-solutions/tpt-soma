# tpt-soma-genomica

Genomic variant annotation and harmonization.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-genomica` takes variants and genes from ingestion and gives them a stable, shared identity: an rsID or genomic coordinate is annotated with its gene, and gene symbols are harmonized to HGNC identifiers. The output is stored so that variant queries and variant-to-expression joins use the same names everywhere.

## Features

- **Variant annotation** (`annotation`): `VariantAnnotation` is the annotation record for one variant. `VariantAnnotationStore` holds them, and `annotate` adds one for a variant ID.
- **Harmonizer** (`Harmonizer`): maps raw variant and gene identifiers to canonical ones. `harmonize_variant` resolves an rsID or coordinate, and `harmonize_gene` resolves a symbol. Mappings are added with `add_rsid_mapping` and `add_gene_mapping`.
- **Pipeline** (`pipeline`): `VariantHarmonizationPipeline` runs an ordered list of steps (`add_step`, `steps`) over a batch of variants. `run` executes the pipeline, and `PipelineError` reports the step that failed.
- **Database backing**: `VariantHarmonizationPipeline::with_database` attaches a Postgres pool and a sample ID, so pipeline results are written to the database. The harmonizer's mappings are held in memory.

Mappings resolve deterministically. A variant with no mapping is left unresolved rather than guessed. [`tpt-soma-harmonize`](../tpt-soma-harmonize/README.md) holds the review queue for such cases.

## Crate layout

| Module | Responsibility |
|---|---|
| `annotation` | `Harmonizer`, `VariantAnnotation`, `VariantAnnotationStore` |
| `pipeline` | `Pipeline`, `VariantHarmonizationPipeline`, `PipelineError`, `run` |

## Running the tests

```sh
cargo test -p tpt-soma-genomica
```

The coverage floor is 60% (see [coverage targets](../../docs/testing/coverage-targets.md)).

## Related crates

- [`tpt-soma-harmonize`](../tpt-soma-harmonize/README.md): the mapping tables and review queue behind unresolved identifiers
- [`tpt-soma-ingest`](../tpt-soma-ingest/README.md): parses the VCF files this crate annotates
- [`tpt-soma-cytos`](../tpt-soma-cytos/README.md): joins variants to expression

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
