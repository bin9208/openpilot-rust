mod carlog;
mod common;
mod frequency_trace;
mod io;
mod monitor;
mod options;
mod params_poll;
mod run;
mod scheduler;
mod setup;
use crate::core;
pub use common::Common;
pub use io::NativeIo;
pub use monitor::Monitor;
pub use options::{parse, Command, RunOptions};
pub use run::run;
pub use setup::{initialize, Initialized};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid card arguments: {0}")]
    Arguments(&'static str),
    #[error("card interrupted")]
    Interrupted,
    #[error(transparent)]
    Core(#[from] core::Error),
    #[error(transparent)]
    Startup(#[from] crate::startup::Error),
    #[error(transparent)]
    Identification(#[from] crate::identification::Error),
    #[error(transparent)]
    Firmware(#[from] crate::firmware::Error),
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Policy(#[from] openpilot_control_policy::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error("card startup setting is not Unicode: {0}")]
    Environment(&'static str),
    #[error("feedforward directory has no JSON models")]
    EmptyModels,
    #[error("source feedforward model requires output_size")]
    ModelOutputSize,
}
