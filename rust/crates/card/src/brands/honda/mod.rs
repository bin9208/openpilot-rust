//! Native Honda interface, preserving opendbc/car/honda source behavior.
mod can;
mod can_ui;
mod config;
mod controller;
mod controller_history;
mod controller_long;
mod controller_math;
mod parameters;
mod parameters_tuning;
mod runtime;
mod state;
mod state_controls;
mod state_motion;
mod state_update;
pub use parameters::{parameters, ParamsInput};
pub use runtime::{Honda, Setup};

pub const BOSCH_EXT_HUD: u32 = 1;
pub const BOSCH_ALT_BRAKE: u32 = 2;
pub const BOSCH: u32 = 4;
pub const BOSCH_RADARLESS: u32 = 8;
pub const NIDEC: u32 = 16;
pub const NIDEC_ALT_PCM_ACCEL: u32 = 32;
pub const NIDEC_ALT_SCM_MESSAGES: u32 = 64;

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
    #[error("Honda numeric conversion")]
    Numeric,
    #[error("unknown Honda platform: {0}")]
    Platform(String),
    #[error("Honda stock message unavailable before state update: {0}")]
    Stock(&'static str),
    #[error("missing Honda copied signal: {0}")]
    Signal(String),
    #[error("Honda integer Params value: {key}")]
    SettingInteger {
        key: &'static str,
        source: crate::brands::hyundai::Error,
    },
}
