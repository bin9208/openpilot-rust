#[cfg(feature = "solver")]
#[allow(unsafe_code)]
pub mod bridge;
#[cfg(feature = "solver")]
mod callbacks;
#[cfg(feature = "solver")]
pub mod estimator;
#[cfg(feature = "solver")]
pub mod kalman;
pub mod model;

pub mod cache;
mod legacy_encoding;
#[cfg(feature = "solver")]
pub mod loop_state;
#[cfg(feature = "native")]
pub mod runtime;
pub mod types;
pub mod wire;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("paramsd contract: {0}")]
    Contract(&'static str),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[cfg(feature = "solver")]
    #[error(transparent)]
    Solver(#[from] cxx::Exception),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Transport(#[from] openpilot_msgq::Error),
    #[cfg(feature = "native")]
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
}
