//! Native product UI policy from selfdrive/ui (MIT); production selection is external.
pub mod cache;
pub mod device;
pub mod params;
pub mod state;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Ui(#[from] openpilot_ui_framework::Error),
    #[error("invalid UI parameter {0}")]
    Parameter(String),
    #[error("UI contract: {0}")]
    Contract(&'static str),
}
