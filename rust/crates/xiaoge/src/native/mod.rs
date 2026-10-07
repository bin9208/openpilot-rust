mod broadcast;
mod http;
pub mod options;
pub mod platform;
pub mod shared;
mod tcp;
mod tesla;
mod vision;
pub use broadcast::run;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    Transport(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Can(#[from] openpilot_can::Error),
    #[error(transparent)]
    Policy(#[from] crate::Error),
    #[error(transparent)]
    Inference(#[from] crate::inference::Error),
    #[error(transparent)]
    OpenCv(#[from] openpilot_opencv_runtime::Error),
    #[error(transparent)]
    Nv12(#[from] crate::nv12::Error),
    #[error(transparent)]
    Jpeg(#[from] openpilot_jpeg::EncodeError),
    #[error(transparent)]
    JpegContract(#[from] openpilot_jpeg::ContractError),
    #[error(transparent)]
    Clock(#[from] openpilot_beepd::Error),
    #[error(transparent)]
    Round(#[from] std::num::ParseFloatError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error("{0}")]
    Contract(&'static str),
}
