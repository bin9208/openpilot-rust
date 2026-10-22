//! Existing PSA dashcam runtime, preserving the source buses and angle policy (#501).
mod controller;
mod parameters;
mod runtime;
pub mod state;
pub use parameters::{parameters, ParamsInput};
pub use runtime::{Psa, Setup};
pub use state::State;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Baseline(#[from] crate::vehicle_params::Error),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Can(#[from] openpilot_can::Error),
    #[error(transparent)]
    Policy(#[from] openpilot_control_policy::Error),
    #[error("unknown PSA platform: {0}")]
    Platform(String),
    #[error("PSA DBC loading failed at {path}: {source}")]
    DbcLoad {
        path: std::path::PathBuf,
        source: openpilot_can::Error,
    },
    #[error("PSA numeric conversion")]
    Numeric,
    #[error("PSA state unavailable before update: {0}")]
    Stock(&'static str),
}
