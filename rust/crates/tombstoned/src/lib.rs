//! Native apport crash-file discovery, reporting and upload-copy lifecycle.
#![forbid(unsafe_code)]
pub mod apport;
pub mod daemon;
pub mod discovery;
pub mod options;
pub mod parse;
mod reader;
pub use parse::safe_fn;

pub const MAX_SIZE: u64 = 100_000_000;
pub const MAX_TOMBSTONE_FN_LEN: usize = 62;
pub const SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Reporting(#[from] openpilot_crash_reporting::Error),
    #[error(transparent)]
    Metadata(#[from] openpilot_runtime_version::Error),
    #[error(transparent)]
    Json(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Decode(#[from] std::str::Utf8Error),
    #[error("source and destination are the same file")]
    SameFile,
    #[error("commit does not support slicing")]
    CommitType,
    #[error("dictionary commit has no slice key")]
    CommitKey,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Posix(#[from] rustix::io::Errno),
    #[error("{0}")]
    Contract(&'static str),
}
