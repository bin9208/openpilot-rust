//! Native continuous hardware daemon. Source: system/hardware/{hardwared,fan_controller,power_monitoring}.py.
pub mod fan;
mod host;
pub mod policy;
pub mod power;
pub mod runtime;
pub mod wire;
mod workers;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Hardware(#[from] openpilot_hardware_info::Error),
    #[error(transparent)]
    Control(#[from] openpilot_hardware_control::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Stats(#[from] openpilot_statsd::Error),
    #[error("{0}")]
    Contract(&'static str),
}
pub fn monotonic() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    now.tv_sec as f64 + now.tv_nsec as f64 / 1e9
}
