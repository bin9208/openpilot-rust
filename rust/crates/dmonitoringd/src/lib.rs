pub mod controller;
mod wire;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("driver monitoring contract: {0}")]
    Contract(&'static str),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Policy(#[from] openpilot_monitoring::InputError),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
