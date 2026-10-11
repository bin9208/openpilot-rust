//! Source-compatible optional webcam capture, pixels and publication.
pub mod selection;

#[cfg(feature = "native")]
pub mod capture;
pub mod pixels;
#[cfg(feature = "native")]
pub mod runtime;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Contract(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
    #[cfg(feature = "native")]
    #[error("OpenCV capture: {0}")]
    Capture(#[from] cxx::Exception),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Scale(#[from] ffmpeg_next::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Vision(#[from] openpilot_msgq::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error("webcam worker: {0}")]
    Worker(String),
    #[error(transparent)]
    Environment(#[from] std::env::VarError),
}
