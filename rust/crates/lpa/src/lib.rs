//! Native SGP.22 LPA port of system/hardware/tici/lpa.py (original MIT provenance).
pub mod at;
pub mod bpp;
pub mod codec;
pub mod http;
pub mod notifications;
pub mod profiles;
pub mod protocol;
pub mod service;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Protocol(String),
    #[error("{0}")]
    Value(String),
    #[error("{0}")]
    Lpa(String),
    #[error("profile not found: {0}")]
    ProfileNotFound(String),
    #[error("AT command timed out")]
    Timeout,
    #[error(transparent)]
    Clock(#[from] openpilot_timed::Error),
    #[error(transparent)]
    Launch(#[from] openpilot_process_supervision::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serial(#[from] serialport::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    HttpRequest(#[from] ureq::http::Error),
    #[error(transparent)]
    Http(#[from] ureq::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn protocol(message: impl Into<String>) -> Error {
    Error::Protocol(message.into())
}
