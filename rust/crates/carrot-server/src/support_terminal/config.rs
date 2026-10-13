use crate::{Error, Value};
use num_traits::ToPrimitive;
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
pub struct Config {
    pub terminal: Arc<crate::terminal::Service>,
    pub web: PathBuf,
    pub data: PathBuf,
    pub repository: PathBuf,
    pub launcher: PathBuf,
    pub params: Option<openpilot_params::Params>,
    pub default_ttl: i64,
    pub default_command_timeout: i64,
    pub public_delay: f64,
    pub shared_http: bool,
}
fn integer(name: &str, default: &str) -> Result<i64, Error> {
    Value::text(&std::env::var(name).unwrap_or_else(|_| default.into()))
        .int()?
        .to_i64()
        .ok_or_else(|| Error::Source(format!("{name} out of range")))
}
impl Config {
    pub fn original(
        app: &crate::http::Application,
        terminal: Arc<crate::terminal::Service>,
    ) -> Result<Self, Error> {
        Ok(Self {
            launcher: terminal.config.launcher.clone(),
            terminal,
            web: app.config.web.clone(),
            data: app
                .config
                .state
                .parent()
                .ok_or_else(|| Error::Source("Carrot data directory missing".into()))?
                .to_path_buf(),
            repository: std::env::var_os("CARROT_REPO_DIR")
                .map_or_else(|| "/data/openpilot".into(), PathBuf::from),
            params: app
                .params
                .lock()
                .map_err(|_| Error::Source("Params lock poisoned".into()))?
                .native_params()
                .cloned(),
            default_ttl: integer("CARROT_SUPPORT_TTL_SECONDS", "1800")?,
            default_command_timeout: integer("CARROT_SUPPORT_COMMAND_TIMEOUT_SECONDS", "30")?,
            public_delay: Value::text(
                &std::env::var("CARROT_SUPPORT_LINK_START_DELAY_SECONDS")
                    .unwrap_or_else(|_| "3.0".into()),
            )
            .float()?,
            shared_http: true,
        })
    }
}
