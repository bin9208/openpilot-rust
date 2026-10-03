mod config;
mod controller;
mod controller_alerts;
mod controller_history;
mod controller_longitudinal;
mod controller_steering;
mod parameters;
mod runtime;
pub mod secoc;
mod state;
mod state_cruise;
mod state_motion;
pub use runtime::{Setup, Toyota};
mod can;
mod static_dsu;
pub use parameters::{parameters, ParamsInput};

pub const HYBRID: u32 = 1;
pub const DISABLE_RADAR: u32 = 4;
pub const TSS2: u32 = 8;
pub const NO_DSU: u32 = 16;
pub const UNSUPPORTED_DSU: u32 = 32;
pub const RADAR_ACC: u32 = 64;
pub const ANGLE_CONTROL: u32 = 128;
pub const NO_STOP_TIMER: u32 = 256;
pub const SNG_WITHOUT_DSU: u32 = 512;
pub const RAISED_ACCEL_LIMIT: u32 = 1024;
pub const SECOC: u32 = 2048;

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
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error("Toyota numeric conversion")]
    Numeric,
    #[error("unknown Toyota platform: {0}")]
    Platform(String),
    #[error("Toyota stock message unavailable before state update: {0}")]
    Stock(&'static str),
    #[error("missing Toyota copied signal: {0}")]
    Signal(String),
    #[error("Toyota SecOC AES key length must be 16, 24, or 32 bytes")]
    KeyLength,
    #[error("Toyota integer setting {key}: {source}")]
    SettingInteger {
        key: &'static str,
        source: crate::brands::hyundai::Error,
    },
}

pub(super) fn eps_scale(candidate: &str) -> u16 {
    match candidate {
        "TOYOTA_PRIUS" => 66,
        "TOYOTA_COROLLA" => 88,
        "LEXUS_IS" | "LEXUS_RC" => 77,
        "LEXUS_CTH" | "TOYOTA_PRIUS_V" => 100,
        _ => 73,
    }
}
