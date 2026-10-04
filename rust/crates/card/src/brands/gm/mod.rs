mod can;
mod can_adas;
mod controller;
mod controller_buttons;
mod controller_long;
mod controller_params;
mod controller_steering;
mod model;
mod parameters;
mod parameters_model;
mod runtime;
mod state;
mod state_controls;
mod state_update;

use num_traits::ToPrimitive;
pub use parameters::{parameters, ParamsInput};
pub use runtime::{Gm, Setup};

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
    Settings(#[from] crate::brands::hyundai::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Policy(#[from] openpilot_control_policy::Error),
    #[error("GM numeric conversion")]
    Numeric,
    #[error("unknown GM platform: {0}")]
    Platform(String),
    #[error("GM stock message unavailable before state update: {0}")]
    Stock(&'static str),
    #[error("missing GM copied signal: {0}")]
    Signal(String),
}
fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
fn integer(value: f64) -> Result<i32, Error> {
    value.to_i32().ok_or(Error::Numeric)
}
