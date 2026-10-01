//! Native policy and sample generation from selfdrive/ui/soundd.py.
pub mod assets;
pub mod policy;
pub mod runtime;
pub mod settings;
mod wave;
pub use policy::{Input, Playback, Policy, Sound};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    TypedParams(#[from] openpilot_params_typed::Error),
    #[error(transparent)]
    Integer(#[from] openpilot_beepd::IntegerError),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Capnp(#[from] capnp::Error),
    #[error(transparent)]
    Audio(#[from] openpilot_portaudio::Error),
    #[error(transparent)]
    Hardware(#[from] openpilot_hardware_info::Error),
    #[error("{0}")]
    Contract(&'static str),
    #[error("missing sound assets: {0}")]
    MissingAsset(String),
}
