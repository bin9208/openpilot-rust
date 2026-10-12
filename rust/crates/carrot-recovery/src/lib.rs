#![forbid(unsafe_code)]
pub mod actions;
pub mod config;
pub mod cwp;
pub mod http;
pub mod support;
pub mod terminal;
pub mod upload;
pub mod websocket;
pub use openpilot_carrot_navi::json::Value;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Source(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] openpilot_carrot_navi::Error),
    #[error(transparent)]
    Process(#[from] openpilot_process_supervision::Error),
}
