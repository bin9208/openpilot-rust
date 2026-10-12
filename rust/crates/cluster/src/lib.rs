//! Source-owned cluster policy and TURZX protocol from issue 253.
pub mod autorun;
pub mod camera;
pub mod config;
pub mod h264;
pub mod jpeg_async;
pub mod live;
pub mod media;
pub mod models;
pub mod monitors;
pub mod navi_simulator;
pub mod rate;
pub mod renderer;
pub mod replay_tools;
pub mod runtime;
pub mod runtime_args;
pub mod runtime_locale;
pub mod runtime_preferences;
pub mod scene;
pub mod simulator;
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
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error("TURZX USB display disconnected")]
    Disconnected(#[source] Box<Error>),
    #[error("{message}")]
    DisplayIo {
        message: &'static str,
        #[source]
        cause: Box<Error>,
    },
}
pub mod route_replay;
