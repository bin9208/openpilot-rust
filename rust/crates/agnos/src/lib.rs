//! Native AGNOS image verification and update runtime. Source: system/hardware/tici/agnos.py.
pub mod cache;
pub mod casync;
pub mod casync_index;
pub mod cli;
pub mod decompress;
pub mod image;
pub mod manifest;
pub mod runtime;
pub mod transport;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Process(#[from] openpilot_process_supervision::Error),
    #[error(transparent)]
    Decompression(#[from] xz2::stream::Error),
    #[error("{class}: {message}")]
    Request {
        class: &'static str,
        message: String,
        transient: bool,
    },
    #[error("{0}")]
    Contract(String),
}
impl Error {
    pub fn connection(message: impl Into<String>) -> Self {
        Self::Request {
            class: "ConnectionError",
            message: message.into(),
            transient: true,
        }
    }
    pub fn transient_download(&self) -> bool {
        matches!(
            self,
            Self::Request {
                transient: true,
                ..
            }
        )
    }
}

pub trait Observer {
    fn log(&mut self, level: &str, text: &str);
    fn progress(&mut self, stage: &str, progress: i64);
    fn sleep(&mut self, seconds: u64);
}
