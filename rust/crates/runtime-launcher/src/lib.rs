#![forbid(unsafe_code)]
pub mod bootstrap;
pub mod handoff;
pub mod native;
pub mod overlay;
pub mod package;
pub mod watchdog;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Process(#[from] openpilot_process_supervision::Error),
    #[error(transparent)]
    Version(#[from] openpilot_runtime_version::Error),
    #[error("runtime launcher: {0}")]
    Configuration(String),
}
