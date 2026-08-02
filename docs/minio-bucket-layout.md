# MinIO Bucket Layout

## Primary Bucket: `raw-omics`

All ingested raw files are stored in the `raw-omics` bucket (configurable via `MINIO_BUCKET`).

| Prefix | Content | Retention |
|--------|---------|-----------|
| `vcf/` | Raw VCF files uploaded via `POST /api/v1/ingest/vcf` | Permanent unless manually deleted |
| `h5ad/` | Raw AnnData `.h5ad` files uploaded via `POST /api/v1/ingest/h5ad` | Permanent unless manually deleted |
| `parquet/` | Exported Parquet files from domain modules | Permanent unless manually deleted |

## Quarantine Bucket: `raw-omics-quarantine`

Files that fail parsing or validation are automatically moved to the quarantine bucket (`{MINIO_BUCKET}-quarantine`).

| Prefix | Content | Retention |
|--------|---------|-----------|
| `vcf/` | VCF files that failed `VcfParser::parse()` | 30 days (manual review required) |
| `h5ad/` | `.h5ad` files that failed `AnnDataParser::parse()` | 30 days (manual review required) |

## Naming Convention

Object keys use UUIDs to avoid collisions:

- `vcf/{uuid}.vcf`
- `h5ad/{uuid}.h5ad`
- `quarantine/vcf/{uuid}.vcf`
- `quarantine/h5ad/{uuid}.h5ad`

## Checksums

All uploads use `upload_with_checksum` which:
1. PUTs the object
2. GETs it back
3. Compares the MD5 of the response body against the client-provided checksum

## Lifecycle

- Ingest endpoints stage uploads to `/tmp` for parsing
- On parse success: file is uploaded to `raw-omics` with checksum verification
- On parse failure: raw bytes are uploaded to `raw-omics-quarantine` and the error response includes the quarantine key
- `/tmp` staging files are always removed after parsing (success or failure)
