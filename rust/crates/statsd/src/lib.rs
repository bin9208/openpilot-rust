//! Native statistics producer and continuous daemon, preserving system/statsd.py.
#![forbid(unsafe_code)]
pub mod aggregation;
pub mod clock;
pub mod daemon;
pub mod events;
mod number;
pub mod producer;
mod sort;
pub const STATS_SOCKET: &str = "ipc:///tmp/stats";
pub const FILE_LIMIT: usize = 10_000;
pub const FLUSH_SECONDS: f64 = 60.0;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Float(#[from] std::num::ParseFloatError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zmq(#[from] zmq::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Metadata(#[from] openpilot_runtime_version::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    TypedParams(#[from] openpilot_params_typed::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error("stats text contains a surrogate")]
    Unicode,
    #[error("{0}")]
    Configuration(&'static str),
}
