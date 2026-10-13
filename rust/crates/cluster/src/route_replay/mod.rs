mod bytes;
mod can;
mod codec;
pub mod corner;
mod frame;
mod metadata;
pub mod overlay;
mod parser;
mod source;
pub use source::{Source, SourceConfig};
pub mod video;
pub use bytes::read_log_bytes;
pub use parser::Parser;
pub mod paths;
pub mod preload;
pub mod timeline;
pub use crate::live::projection::LaneSnapshot;
pub use frame::{Frame, Metadata};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Process(#[from] openpilot_process_supervision::Error),
    #[error("{0}")]
    Invalid(String),
}
