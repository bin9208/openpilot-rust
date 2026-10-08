//! Original Carrot Web server policies from openpilot/selfdrive/carrot/server (#225).
pub mod config;
pub mod http;
mod http_request;
mod http_response;
mod http_server;
mod json_fields;
mod native;
mod param_coercion;
pub mod params;
pub mod settings;
mod settings_brand;
mod settings_cache;
mod settings_menu;
pub mod static_assets;

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
    Params(#[from] openpilot_params::Error),
}
