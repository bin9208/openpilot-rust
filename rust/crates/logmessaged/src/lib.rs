//! Original logmessaged record and rotating Swaglog file behavior.
mod filename;
mod format;
mod json;
mod rotation;
pub use format::format_record;
pub use json::{Error as JsonError, JsonValue, JsonView};
pub use rotation::{LogFiles, RotationSettings};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("log file I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("log record format: {0}")]
    Format(#[from] json::Error),
    #[error("log formatting failed: {0}")]
    Write(#[from] std::fmt::Error),
    #[error("log random identifier: {0}")]
    Random(getrandom::Error),
    #[error("log handler stream is closed after a failed rollover")]
    Closed,
    #[error("invalid numeric log file suffix")]
    FileIndex,
}
pub mod options;
pub mod wire;
