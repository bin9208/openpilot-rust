//! Project-owned locationd/lagd.py port; original licensing and source provenance apply.
pub mod blocks;
pub mod correlation;
pub mod estimate;
pub mod points;
pub mod pose;
pub mod smoothing;
mod sum;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Kernel(#[from] cxx::Exception),
    #[error("lagd contract: {0}")]
    Contract(&'static str),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Text(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Platform(#[from] openpilot_torqued::Error),
}

pub mod estimator;
pub mod message;
pub mod motion;
pub mod settings;

pub mod loop_state;
pub mod parameters;
pub mod wire;
