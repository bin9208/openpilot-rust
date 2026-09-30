#![forbid(unsafe_code)]

mod detached_child;
mod ensure;
mod exec;
mod launch;
mod logging;
mod persistent;
mod pid;
mod process;
mod state;

pub use ensure::ensure_running;
pub use launch::{run_child, CapturedChild, CapturedCommand, NativeCommand};
pub use logging::ProcessLog;
pub use persistent::{ParamsSource, PersistentCommand, PersistentDaemonProcess};
pub use process::{Execution, ManagedProcess, ProcessPolicy, StopOptions};
pub use rustix::process::Signal;
pub use state::ProcessState;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Nul(#[from] std::ffi::NulError),
    #[error("process command line is empty")]
    EmptyCommand,
    #[error("process ID exceeds the signed OS PID range")]
    PidRange,
    #[error("Params key {0} is not an INT PID key")]
    PidKeyType(String),
    #[error("persistent Params initialization missing")]
    MissingParams,
    #[error("persistent child registry lock poisoned")]
    ReaperPoisoned,
    #[error("native child launch protocol: {0}")]
    LaunchProtocol(&'static str),
}
