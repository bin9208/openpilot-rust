//! Source-owned cluster policy and TURZX protocol from issue 253.
pub mod autorun;
pub mod rate;
pub mod uevent;
pub mod usb;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Contract(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Usb(#[from] openpilot_usbgpu::Error),
    #[error("TURZX USB display disconnected")]
    Disconnected(#[source] Box<Error>),
    #[error("{message}")]
    DisplayIo {
        message: &'static str,
        #[source]
        cause: Box<Error>,
    },
}
