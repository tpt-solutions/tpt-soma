use std::process::Command;

pub struct ScanpyOrchestrator {
    pub script_path: String,
}

impl ScanpyOrchestrator {
    pub fn new(script_path: impl Into<String>) -> Self {
        Self { script_path: script_path.into() }
    }

    pub fn run(&self, input: &str, output: &str) -> Result<(), ScanpyError> {
        let status = Command::new("python")
            .arg(&self.script_path)
            .arg("--input").arg(input)
            .arg("--output").arg(output)
            .status()?;
        if !status.success() {
            return Err(ScanpyError::ExecutionFailed);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScanpyError {
    #[error("execution failed")]
    ExecutionFailed,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
