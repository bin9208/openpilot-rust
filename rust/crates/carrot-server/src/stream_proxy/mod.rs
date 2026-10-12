mod history;
mod http;
mod summary;
use crate::Error;
pub use history::History;
pub use http::handle;
use std::time::Duration;

pub struct Service {
    target: String,
    client: reqwest::Client,
    params: Option<openpilot_params::Params>,
    pub history: History,
}
impl Service {
    pub fn new(target: String, params: Option<openpilot_params::Params>) -> Result<Self, Error> {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| Error::Source(error.to_string()))?;
        Ok(Self {
            target,
            client,
            params,
            history: History::default(),
        })
    }
    pub fn original(params: Option<openpilot_params::Params>) -> Result<Self, Error> {
        Self::new("http://127.0.0.1:5001/stream".into(), params)
    }
    fn cluster_active(&self) -> bool {
        self.params.as_ref().is_some_and(|params| {
            let bytes = params.get("ClusterHud").ok().flatten().unwrap_or_default();
            match openpilot_beepd::integer(&bytes) {
                Ok(value) => value == 1,
                Err(error) => crate::param_native::fatal("ClusterHud", &error),
            }
        })
    }
}
