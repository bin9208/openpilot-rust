pub mod driver;
pub mod runtime;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error("driver daemon I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("driver daemon IPC: {0}")]
    Ipc(#[from] openpilot_msgq::Error),
    #[error("driver daemon model: {0}")]
    Model(#[from] openpilot_model_runtime::Error),
    #[error("driver daemon cereal: {0}")]
    Cereal(#[from] capnp::Error),
    #[error("driver daemon output: {0}")]
    Output(#[from] openpilot_modeld::parse::ParseError),
    #[error("driver daemon contract: {0}")]
    Contract(&'static str),
}
