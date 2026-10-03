mod can;
mod controller;
mod controller_alerts;
mod controller_cruise;
mod parameters;
mod runtime;
mod state;
mod state_cruise;
mod state_motion;
pub use parameters::{parameters, ParamsInput};
pub use runtime::{Setup, Subaru};

pub const SEND_INFOTAINMENT: u32 = 1;
pub const DISABLE_EYESIGHT: u32 = 2;
pub const GLOBAL_GEN2: u32 = 4;
pub const STEER_RATE_LIMITED: u32 = 8;
pub const PREGLOBAL: u32 = 16;
pub const HYBRID: u32 = 32;
pub const LKAS_ANGLE: u32 = 64;

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
    #[error("Subaru numeric conversion")]
    Numeric,
    #[error("unknown Subaru platform: {0}")]
    Platform(String),
    #[error("Subaru stock message unavailable before state update: {0}")]
    Stock(&'static str),
    #[error("missing Subaru copied signal: {0}")]
    Signal(String),
}
