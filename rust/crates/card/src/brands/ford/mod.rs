mod can;
mod can_ui;
mod controller;
mod limits;
mod parameters;
mod runtime;
mod state;
mod state_update;

pub use parameters::{parameter_logs, parameters, parameters_logged, ParamsInput};
pub use runtime::{Ford, Setup};

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
    #[error("Ford numeric conversion")]
    Numeric,
    #[error("unknown Ford platform: {0}")]
    Platform(String),
    #[error("Ford stock message unavailable before state update: {0}")]
    Stock(&'static str),
    #[error("missing Ford copied signal: {0}")]
    Signal(String),
}
