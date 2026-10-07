pub mod association;
pub mod controller;
pub mod daemon;
pub mod lead;
pub mod math;
pub mod model;
pub mod path;
pub mod point;
pub mod predictor;
pub mod primary;
#[cfg(feature = "native-skip-miri")]
pub mod runtime;
pub mod scope;
pub mod selection;
pub mod trajectory_cutin;
pub mod trajectory_cutout;
pub mod wire;
pub mod wire_float;

pub use openpilot_plannerd::lead::Lead;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    MessagingState(#[from] openpilot_messaging::state::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[cfg(feature = "native-skip-miri")]
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("radard parameter: {0}")]
    Parameter(String),
    #[error("radard interrupted by signal {0}")]
    Signal(i32),
    #[error("{0}")]
    Contract(&'static str),
    #[error(transparent)]
    Radar(#[from] openpilot_radarcan::Error),
}
