//! Native product UI policy from selfdrive/ui (MIT); production selection is external.
pub mod api;
pub mod cache;
pub mod context;
pub mod device;
pub mod layouts;
pub mod mici;
pub mod onroad;
pub mod paint;
pub mod params;
pub mod qr;
pub mod render_diagnostics;
#[path = "main/mod.rs"]
pub mod root_layout;
pub mod runtime;
pub mod scheduling;
pub mod services;
pub mod settings;
pub mod state;
pub mod vision_status;
pub mod widgets;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Text(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Ui(#[from] openpilot_ui_framework::Error),
    #[error("invalid UI parameter {0}")]
    Parameter(String),
    #[error("UI contract: {0}")]
    Contract(&'static str),
}

impl From<Error> for openpilot_startup_ui::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Ui(error) => error,
            error => Self::Io(std::io::Error::other(error)),
        }
    }
}
