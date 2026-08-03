use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanpyError {
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("script not found: {0}")]
    ScriptNotFound(String),
}

pub struct ScanpyOrchestrator {
    pub script_path: String,
}

impl ScanpyOrchestrator {
    pub fn new(script_path: impl Into<String>) -> Self {
        Self {
            script_path: script_path.into(),
        }
    }

    pub fn run(&self, input: &str, output: &str) -> Result<ScanpyResult, ScanpyError> {
        if !std::path::Path::new(&self.script_path).exists() {
            return Err(ScanpyError::ScriptNotFound(self.script_path.clone()));
        }

        let status = Command::new("python")
            .arg(&self.script_path)
            .arg("--input")
            .arg(input)
            .arg("--output")
            .arg(output)
            .status()?;

        if !status.success() {
            return Err(ScanpyError::ExecutionFailed(format!(
                "Scanpy script exited with status: {}",
                status
            )));
        }

        Ok(ScanpyResult {
            output_path: output.to_string(),
        })
    }
}

#[derive(Debug)]
pub struct ScanpyResult {
    pub output_path: String,
}

pub struct ScanpyScriptGenerator;

impl ScanpyScriptGenerator {
    pub fn generate_preprocessing_script(output_path: &str) -> Result<(), std::io::Error> {
        let script = r#"
import scanpy as sc
import argparse
import os

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    # Read AnnData
    adata = sc.read_h5ad(args.input)

    # Basic preprocessing
    sc.pp.filter_cells(adata, min_genes=200)
    sc.pp.filter_genes(adata, min_cells=3)
    sc.pp.normalize_total(adata, target_sum=1e4)
    sc.pp.log1p(adata)
    sc.pp.highly_variable_genes(adata, n_top_genes=2000)
    adata = adata[:, adata.var.highly_variable]
    sc.pp.scale(adata, max_value=10)
    sc.tl.pca(adata, svd_solver='arpack')
    sc.pp.neighbors(adata, n_neighbors=10, n_pcs=40)
    sc.tl.umap(adata)
    sc.tl.leiden(adata, resolution=0.5)

    # Save results
    os.makedirs(args.output, exist_ok=True)
    adata.write_h5ad(os.path.join(args.output, "processed.h5ad"))
    
    # Save UMAP coordinates
    umap_df = adata.obsm['X_umap']
    import pandas as pd
    umap_df = pd.DataFrame(umap_df, columns=['UMAP1', 'UMAP2'], index=adata.obs_names)
    umap_df.to_csv(os.path.join(args.output, "umap_coords.csv"))
    
    # Save cluster labels
    clusters = adata.obs['leiden']
    clusters.to_csv(os.path.join(args.output, "cluster_labels.csv"))

if __name__ == "__main__":
    main()
"#;
        std::fs::write(output_path, script)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_generation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let script_path = temp_dir.path().join("preprocess.py");
        ScanpyScriptGenerator::generate_preprocessing_script(script_path.to_str().unwrap())
            .unwrap();
        assert!(script_path.exists());
        let content = std::fs::read_to_string(script_path).unwrap();
        assert!(content.contains("scanpy"));
        assert!(content.contains("umap"));
        assert!(content.contains("leiden"));
    }

    #[test]
    fn test_orchestrator_missing_script() {
        let orchestrator = ScanpyOrchestrator::new("/definitely/not/a/real/script.py");
        let err = orchestrator.run("in.h5ad", "out").unwrap_err();
        assert!(matches!(err, ScanpyError::ScriptNotFound(_)));
    }
}
