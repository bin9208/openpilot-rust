//! Native manager policy and runtime. Installation adapters explicitly own startup UI,
//! hardware actions and complete daemon selection; no Python fallback is provided.
#![forbid(unsafe_code)]
pub mod boot_lock;
pub mod initialization;
pub mod lifecycle;
pub mod main_loop;
pub mod native_boot;
pub mod native_exit;
pub mod parameters;
pub mod processes;
pub mod runtime;
pub mod signals;
pub mod startup;
pub mod supported_cars;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Catalog(#[from] openpilot_manager_catalog::Error),
    #[error(transparent)]
    Process(#[from] openpilot_process_supervision::Error),
    #[error(transparent)]
    Version(#[from] openpilot_runtime_version::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Registration(#[from] openpilot_registration::Error),
    #[error(transparent)]
    Reporting(#[from] openpilot_crash_reporting::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Bootlog(#[from] openpilot_bootlog::snapshot::Error),
    #[error(transparent)]
    Hardware(#[from] openpilot_hardware_control::Error),
    #[error("manager contract: {0}")]
    Contract(&'static str),
    #[error("native daemon unavailable: {0}")]
    Unavailable(String),
    #[error("manager interrupted")]
    Interrupted,
}
