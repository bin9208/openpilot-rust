//! Original Carrot Web server policies from openpilot/selfdrive/carrot/server (#225).
mod bootstrap;
pub mod cars;
pub mod config;
mod history_http;
pub mod http;
mod http_request;
mod http_response;
mod http_server;
pub mod intro;
mod json_fields;
mod native;
pub mod param_changes;
mod param_coercion;
mod param_native;
pub mod param_qr;
pub mod param_restore;
mod param_time;
pub mod params;
mod params_http;
mod profiles_http;
mod request_text;
mod restore_http;
pub mod setting_profiles;
pub mod settings;
mod settings_brand;
mod settings_cache;
mod settings_menu;
mod state;
mod state_http;
pub(crate) mod state_json;
mod state_preferences;
pub mod static_assets;
pub mod static_web;
pub mod web_settings;
mod web_settings_http;

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
