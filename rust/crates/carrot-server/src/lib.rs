//! Original Carrot Web server policies from openpilot/selfdrive/carrot/server (#225).
pub mod bluetooth_http;
mod bootstrap;
pub mod cars;
pub mod config;
pub mod dashcam;
pub mod egpu_model;
pub mod git_state;
pub mod git_status;
mod history_http;
pub mod http;
mod http_request;
mod http_response;
mod http_routes;
mod http_server;
pub mod intro;
mod json_fields;
pub mod mapbox_tokens;
mod native;
pub mod param_changes;
mod param_coercion;
mod param_native;
pub mod param_qr;
pub mod param_restore;
mod param_time;
pub mod params;
mod params_http;
pub mod params_multipart;
pub mod popular_values;
mod profiles_http;
pub(crate) mod request_body;
mod request_text;
mod restore_http;
pub mod screenrecord;
pub mod setting_profiles;
pub mod settings;
mod settings_brand;
mod settings_cache;
mod settings_menu;
pub mod ssh_keys;
mod state;
mod state_http;
pub(crate) mod state_json;
mod state_preferences;
pub mod static_assets;
pub mod static_web;
#[cfg(test)]
mod transport_test;
pub mod web_settings;
mod web_settings_http;
pub mod web_sound;
mod web_sound_http;
pub mod xiaoge;

pub use openpilot_carrot_navi::json::Value;
pub use request_body::DecodeFailure;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Source(String),
    #[error("unknown encoding: {0}")]
    UnknownCharset(String),
    #[error(transparent)]
    Request(#[from] DecodeFailure),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] openpilot_carrot_navi::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
}
