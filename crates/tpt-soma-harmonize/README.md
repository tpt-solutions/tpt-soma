# tpt-soma-harmonize

Deterministic ontology mapping (dbSNP rsID, HGNC, LOINC, SNOMED) with human-in-the-loop review.

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-harmonize` makes sure that the same clinical or genomic concept is always stored under the same code. It holds mapping tables from local terms to standard ontologies, resolves incoming terms against them, and queues anything it cannot map confidently for a person to review. A mapping only changes when a reviewer changes it.

## Features

- **Mapping tables** (`mapping`): `MappingTable` holds `MappingEntry` rows from a source term to a target code. Each entry is tagged with an `OntologySource`: dbSNP rsIDs, HGNC gene symbols, LOINC observation codes, SNOMED CT concepts or UBERON anatomy terms. `with_confidence` and `with_notes` record how a mapping was made, and `seed_phase1_phase2` loads the initial tables.
- **Deterministic resolution**: `resolve` and `resolve_entry` look up a term in the table and return its mapping. The same input always gives the same result, with no fuzzy matching in the resolution path.
- **Review queue** (`review`): `ReviewQueue` collects `Unmapped` terms that did not resolve. `load_queue` and `save_queue` persist it, and reviewed decisions come back in through the CSV import.
- **CSV I/O** (`io`): `import_csv_mappings` loads reviewed mappings in bulk, and `export_queue_to_csv` writes the queue out for a reviewer. The CSV round trip is covered by tests.
- **Persistence**: `load_mapping_table` and `save_mapping_table` store tables in Postgres. `insert`, `insert_simple` and `get_by_source` give direct access.
- **Review CLI** (`review-cli`): an operator tool for working the queue in a terminal.

## Human-in-the-loop review

1. A term arrives from ingestion or an observation feed.
2. `resolve` looks it up. If it matches, the mapped code is used.
3. If not, the term is added to the `ReviewQueue` as `Unmapped`.
4. A reviewer maps it with `review-cli` or by exporting, editing and re-importing the CSV.
5. The new mapping is saved with its confidence and notes, so the decision is auditable.

Resolution never invents a mapping. Entries reach the table through `seed_phase1_phase2` or a CSV import of reviewed decisions.

## Crate layout

| Module | Responsibility |
|---|---|
| `mapping` | `MappingTable`, `MappingEntry`, `OntologySource`, resolution |
| `review` | `ReviewQueue`, `Unmapped`, queue persistence |
| `io` | `import_csv_mappings`, `export_queue_to_csv` |
| `bin/review_cli.rs` | The `review-cli` binary |

Errors are reported as `HarmonizeError`: mapping, review, database, I/O, serialization and CSV failures.

## Running the tests

```sh
cargo test -p tpt-soma-harmonize
```

The coverage floor is 75% (see [coverage targets](../../docs/testing/coverage-targets.md)).

## Related crates

- [`tpt-soma-ingest`](../tpt-soma-ingest/README.md): produces the terms that get harmonized
- [`tpt-soma-genomica`](../tpt-soma-genomica/README.md): variant and gene identifiers
- [`tpt-soma-organon`](../tpt-soma-organon/README.md): LOINC-coded clinical observations

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
