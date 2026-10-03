mod can;
mod controller;
mod parameters;
mod runtime;
mod state;

pub use parameters::{parameters, ParamsInput};
pub use runtime::{Rivian, Setup};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Can(#[from] openpilot_can::Error),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Baseline(#[from] crate::vehicle_params::Error),
    #[error(transparent)]
    Policy(#[from] openpilot_control_policy::Error),
    #[error("Rivian numeric conversion")]
    Numeric,
    #[error("unknown Rivian platform: {0}")]
    Platform(String),
    #[error("Rivian stock message unavailable before state update: {0}")]
    Stock(&'static str),
    #[error("missing Rivian copied signal: {0}")]
    Signal(String),
}
