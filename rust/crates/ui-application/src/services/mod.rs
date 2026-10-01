pub mod firehose;
pub mod polling;
pub mod prime;
pub mod ssh;
use crate::api::{self, http, TokenCache, TokenPaths};
use openpilot_timed::clock::{Clock, SystemClock};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Params(#[from] crate::Error),
    #[error(transparent)]
    RawParams(#[from] openpilot_params::Error),
    #[error(transparent)]
    Token(#[from] api::Error),
    #[error(transparent)]
    Http(#[from] http::Error),
    #[error(transparent)]
    Version(#[from] openpilot_runtime_version::Error),
    #[error(transparent)]
    Json(#[from] openpilot_logmessaged::JsonError),
    #[error("UI API contract: {0}")]
    Contract(&'static str),
}
#[derive(Clone)]
pub struct Api {
    pub host: String,
    pub source_root: PathBuf,
    pub persist: PathBuf,
    pub systemd: PathBuf,
    pub clock: Arc<dyn Clock + Send + Sync>,
    cache: Arc<Mutex<TokenCache>>,
}
impl Api {
    pub fn new(host: String, source_root: PathBuf, persist: PathBuf) -> Self {
        Self {
            host,
            source_root,
            persist,
            systemd: "/lib/systemd/systemd".into(),
            clock: Arc::new(SystemClock),
            cache: Arc::default(),
        }
    }
    pub fn session(&self, timeout: Option<Duration>) -> Result<http::Session, Error> {
        Ok(http::Session::new(
            format!(
                "openpilot-{}",
                openpilot_runtime_version::get_version(&self.source_root)?
            ),
            timeout,
        ))
    }
    pub fn token(&self, identity: &str) -> Result<String, Error> {
        Ok(self
            .cache
            .lock()
            .map_err(|_| Error::Contract("token cache poisoned"))?
            .get(
                identity,
                self.clock.as_ref(),
                TokenPaths {
                    persist: &self.persist,
                    systemd: &self.systemd,
                },
            )?)
    }
}
pub fn truthy(value: &openpilot_logmessaged::JsonValue) -> bool {
    use openpilot_logmessaged::JsonView;
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value != "0",
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}

pub mod updater;
