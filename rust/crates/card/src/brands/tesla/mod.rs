mod controller;
mod parameters;
mod runtime;
pub mod speed_limit;
pub mod state;
mod steering;
pub use parameters::{parameters, ParamsInput};
pub use runtime::{Setup, Tesla};

pub const FSD_14: u32 = 2;
pub const MISSING_DAS_SETTINGS: u32 = 4;
pub const HAS_VEHICLE_BUS: u32 = 8;
pub const AUTO_SPEED_LIMIT: u32 = 16;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Can(#[from] openpilot_can::Error),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Baseline(#[from] crate::vehicle_params::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Policy(#[from] openpilot_control_policy::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error("Tesla numeric conversion")]
    Numeric,
    #[error("Tesla speed-wheel template must be an idle 0x3C2 mux-1 frame")]
    WheelTemplate,
    #[error("Tesla speed-wheel tick must be -1 or +1")]
    WheelTick,
}
