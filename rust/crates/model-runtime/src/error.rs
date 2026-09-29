#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid model manifest: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported model contract: {0}")]
    Contract(&'static str),
    #[error("invalid {kind} at index {index}")]
    Invalid { kind: &'static str, index: usize },
    #[error("model exceeds resource limit: {0}")]
    Limit(&'static str),
    #[error("model I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("GPU execution failed: {execution}; cleanup also failed: {cleanup}")]
    GpuCleanup {
        execution: std::io::Error,
        cleanup: std::io::Error,
    },
    #[error("model asset checksum mismatch: {0}")]
    Checksum(&'static str),
    #[error("model buffer allocation failed")]
    Allocation,
    #[error("unknown model binding: {0}")]
    Binding(String),
    #[error("unknown model entrypoint: {0}")]
    Entrypoint(String),
    #[error("model binding requires {expected} bytes, received {actual}")]
    Size { expected: usize, actual: usize },
    #[cfg(feature = "native-skip-miri")]
    #[error("native kernel library: {0}")]
    Library(#[from] libloading::Error),
}
