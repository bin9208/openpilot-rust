use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reporter stopped")]
    Stopped,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error("{0}")]
    Contract(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub report_url: String,
    pub heartbeat_url: String,
    pub iface: String,
    pub port: i64,
    pub timeout_s: f64,
    pub heartbeat_interval_s: f64,
    pub debounce_s: f64,
    pub dry_run: bool,
}
impl Default for Config {
    fn default() -> Self {
        let report_url = crate::helpers::default_url();
        Self {
            heartbeat_url: crate::helpers::heartbeat_url(&report_url),
            report_url,
            iface: "wlan0".into(),
            port: 7000,
            timeout_s: 4.,
            heartbeat_interval_s: 10.,
            debounce_s: 1.,
            dry_run: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Payload {
    #[serde(rename = "deviceId")]
    pub device_id: String,
    pub ip: String,
    pub port: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PostResult {
    pub ok: bool,
    pub status: u16,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusKind {
    NoIp,
    IpCandidate,
    HeartbeatDryRun,
    Heartbeat,
    HeartbeatFailed,
    Idle,
    DryRun,
    Reported,
    ReportFailed,
    OnceNoReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub state: StatusKind,
    pub ts: i64,
    pub last_success_ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Payload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_in_s: Option<f64>,
}
impl Status {
    pub fn ip(mut self, value: &str) -> Self {
        self.ip = Some(value.into());
        self
    }
    pub fn payload(mut self, value: Payload) -> Self {
        self.payload = Some(value);
        self
    }
    pub fn response(mut self, result: PostResult) -> Self {
        self.http_status = Some(result.status);
        self.response = Some(result.body.chars().take(240).collect());
        self
    }
    pub fn failure(mut self, result: PostResult, retry: f64) -> Self {
        self.http_status = Some(result.status);
        self.error = Some(result.body.chars().take(240).collect());
        self.retry_in_s = Some(retry);
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct State {
    pub last_success_ip: String,
    pub current_candidate_ip: String,
    pub current_candidate_since: f64,
    pub was_down: bool,
    pub first_report: bool,
    pub next_retry_at: f64,
    pub backoff_s: f64,
    pub next_heartbeat_at: f64,
}

pub trait Platform {
    fn monotonic(&self) -> f64;
    fn wall_seconds(&self) -> f64;
    fn uniform(&mut self, low: f64, high: f64) -> f64;
    fn local_ip(&mut self, iface: &str) -> String;
    fn device_id(&mut self) -> String;
    fn post(&mut self, url: &str, payload: &Payload, timeout_s: f64) -> Result<PostResult, Error>;
    fn emit(&mut self, status: Status) -> Result<(), Error>;
}
