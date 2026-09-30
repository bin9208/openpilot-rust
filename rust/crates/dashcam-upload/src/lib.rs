pub mod catalog;
mod engine;
pub mod manager;
pub mod metadata;
pub mod report;
pub mod state;
pub mod worker;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{text}")]
    Http { status: u16, text: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Upload(#[from] openpilot_web_upload::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Phase(#[from] state::InvalidPhase),
    #[error("upload canceled")]
    Canceled,
    #[error("{0}")]
    Runtime(String),
}
