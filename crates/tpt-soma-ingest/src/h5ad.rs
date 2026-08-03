use hdf5_reader::{Dataset, Hdf5File};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum H5adError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HDF5 error: {0}")]
    Hdf5(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("unsupported h5ad layout: {0}")]
    UnsupportedLayout(String),
}

pub struct AnnDataParser<'a> {
    path: &'a str,
}

impl<'a> AnnDataParser<'a> {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }

    pub fn parse(&self) -> Result<AnnDataResult, H5adError> {
        let file = Hdf5File::open(self.path).map_err(|e| H5adError::Hdf5(e.to_string()))?;

        let cell_ids = Self::read_string_dataset(&file, "obs/_index")
            .or_else(|_| Self::read_string_dataset(&file, "obs/index"))?;
        let gene_ids = Self::read_string_dataset(&file, "var/_index")
            .or_else(|_| Self::read_string_dataset(&file, "var/index"))?;

        let n_cells = cell_ids.len();
        let n_genes = gene_ids.len();

        let records = Self::read_expression_matrix(&file, &cell_ids, &gene_ids)?;

        Ok(AnnDataResult {
            metadata: ScRNASeqMetadata {
                sample_id: String::new(),
                n_cells,
                n_genes,
                cell_ids,
                gene_ids,
            },
            records,
        })
    }

    fn read_string_dataset(file: &Hdf5File, path: &str) -> Result<Vec<String>, H5adError> {
        let dataset = file
            .dataset(path)
            .map_err(|e| H5adError::Hdf5(e.to_string()))?;

        dataset
            .read_strings()
            .map_err(|e| H5adError::Hdf5(e.to_string()))
    }

    fn read_expression_matrix(
        file: &Hdf5File,
        cell_ids: &[String],
        gene_ids: &[String],
    ) -> Result<Vec<ScRNASeqRecord>, H5adError> {
        let x_group = match file.group("X") {
            Ok(g) => g,
            Err(_) => return Ok(Vec::new()),
        };

        if let Ok(_data_ds) = x_group.dataset("data") {
            let indices = x_group
                .dataset("indices")
                .map_err(|e| H5adError::Hdf5(e.to_string()))?;
            let indptr = x_group
                .dataset("indptr")
                .map_err(|e| H5adError::Hdf5(e.to_string()))?;

            let indices_raw = indices
                .read_raw_bytes()
                .map_err(|e| H5adError::Hdf5(e.to_string()))?;
            let indptr_raw = indptr
                .read_raw_bytes()
                .map_err(|e| H5adError::Hdf5(e.to_string()))?;

            let col_indices: Vec<u32> = Self::decode_int_array(&indices_raw)?;
            let row_ptr: Vec<usize> = Self::decode_usize_array(&indptr_raw)?;

            let mut records = Vec::new();
            for (row_idx, cell_id) in cell_ids.iter().enumerate() {
                let start = row_ptr.get(row_idx).copied().unwrap_or(0);
                let end = row_ptr
                    .get(row_idx + 1)
                    .copied()
                    .unwrap_or(col_indices.len());
                for &col_idx in &col_indices[start..end] {
                    let gene_id = gene_ids.get(col_idx as usize).cloned().unwrap_or_default();
                    records.push(ScRNASeqRecord {
                        sample_id: String::new(),
                        cell_id: cell_id.clone(),
                        gene_id,
                        count: 1,
                    });
                }
            }
            Ok(records)
        } else if let Ok(dense_ds) = x_group.dataset("X") {
            Self::read_dense_matrix(&dense_ds, cell_ids, gene_ids)
        } else {
            Err(H5adError::UnsupportedLayout(
                "X group has neither data nor X dataset".to_string(),
            ))
        }
    }

    fn read_dense_matrix(
        dense_ds: &Dataset,
        cell_ids: &[String],
        gene_ids: &[String],
    ) -> Result<Vec<ScRNASeqRecord>, H5adError> {
        let values: Vec<f64> = dense_ds
            .read_array::<f64>()
            .map_err(|e| H5adError::Hdf5(e.to_string()))?
            .as_slice()
            .ok_or_else(|| H5adError::Hdf5("dense matrix not contiguous".to_string()))?
            .to_vec();

        let n_cells = cell_ids.len();
        let n_genes = gene_ids.len();

        if values.len() != n_cells * n_genes {
            return Err(H5adError::Parse(format!(
                "dense matrix size {} does not match {} cells x {} genes",
                values.len(),
                n_cells,
                n_genes
            )));
        }

        let mut records = Vec::new();
        for (i, &val) in values.iter().enumerate() {
            if val == 0.0 {
                continue;
            }
            let row_idx = i / n_genes;
            let col_idx = i % n_genes;
            records.push(ScRNASeqRecord {
                sample_id: String::new(),
                cell_id: cell_ids[row_idx].clone(),
                gene_id: gene_ids[col_idx].clone(),
                count: val.round() as u32,
            });
        }

        Ok(records)
    }

    fn decode_int_array(raw: &[u8]) -> Result<Vec<u32>, H5adError> {
        // Check for 8-byte first since 8-byte lengths are also multiples of 4
        let elem_size = if raw.len().is_multiple_of(8) {
            8
        } else if raw.len().is_multiple_of(4) {
            4
        } else {
            8
        };
        let n = raw.len() / elem_size;
        let mut result = Vec::with_capacity(n);
        for i in 0..n {
            let offset = i * elem_size;
            let bytes = &raw[offset..offset + elem_size];
            let val = match elem_size {
                4 => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                8 => u64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]) as u32,
                _ => {
                    return Err(H5adError::UnsupportedLayout(format!(
                        "unsupported integer size: {}",
                        elem_size
                    )));
                }
            };
            result.push(val);
        }
        Ok(result)
    }

    fn decode_usize_array(raw: &[u8]) -> Result<Vec<usize>, H5adError> {
        let elem_size = if raw.len().is_multiple_of(8) { 8 } else { 4 };
        let n = raw.len() / elem_size;
        let mut result = Vec::with_capacity(n);
        for i in 0..n {
            let offset = i * elem_size;
            let bytes = &raw[offset..offset + elem_size];
            let val: usize = match elem_size {
                8 => u64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]) as usize,
                4 => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize,
                _ => {
                    return Err(H5adError::UnsupportedLayout(format!(
                        "unsupported indptr size: {}",
                        elem_size
                    )));
                }
            };
            result.push(val);
        }
        Ok(result)
    }
}

pub struct AnnDataResult {
    pub metadata: ScRNASeqMetadata,
    pub records: Vec<ScRNASeqRecord>,
}

#[derive(Debug, Clone)]
pub struct ScRNASeqRecord {
    pub sample_id: String,
    pub cell_id: String,
    pub gene_id: String,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct ScRNASeqMetadata {
    pub sample_id: String,
    pub n_cells: usize,
    pub n_genes: usize,
    pub cell_ids: Vec<String>,
    pub gene_ids: Vec<String>,
}

impl AnnDataResult {
    pub fn n_cells(&self) -> usize {
        self.metadata.n_cells
    }

    pub fn n_genes(&self) -> usize {
        self.metadata.n_genes
    }

    pub fn n_records(&self) -> usize {
        self.records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_int_array_4_byte() {
        // [1u32, 2u32, 3u32] little-endian
        let raw: Vec<u8> = vec![1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0];
        assert_eq!(
            AnnDataParser::decode_int_array(&raw).unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn decode_int_array_8_byte() {
        // [1u64, 300u64] little-endian
        let mut raw: Vec<u8> = Vec::new();
        raw.extend_from_slice(&1u64.to_le_bytes());
        raw.extend_from_slice(&300u64.to_le_bytes());
        assert_eq!(AnnDataParser::decode_int_array(&raw).unwrap(), vec![1, 300]);
    }

    #[test]
    fn decode_usize_array_4_byte() {
        // 5 entries (20 bytes) is not divisible by 8, so width is 4
        let raw: Vec<u8> = vec![0, 2, 4, 5, 7]
            .into_iter()
            .flat_map(|v| (v as u32).to_le_bytes())
            .collect();
        assert_eq!(
            AnnDataParser::decode_usize_array(&raw).unwrap(),
            vec![0, 2, 4, 5, 7]
        );
    }

    #[test]
    fn decode_usize_array_8_byte() {
        let mut raw: Vec<u8> = Vec::new();
        raw.extend_from_slice(&0usize.to_le_bytes());
        raw.extend_from_slice(&7usize.to_le_bytes());
        assert_eq!(AnnDataParser::decode_usize_array(&raw).unwrap(), vec![0, 7]);
    }

    #[test]
    fn decode_int_array_truncated_returns_empty() {
        // The width heuristic is length-based only, so a truncated (3-byte)
        // buffer decodes as 0 elements of the inferred width. Documenting the
        // current lenient behavior rather than silently assuming an error.
        assert_eq!(
            AnnDataParser::decode_int_array(&[1, 2, 3]).unwrap(),
            Vec::<u32>::new()
        );
    }
}
