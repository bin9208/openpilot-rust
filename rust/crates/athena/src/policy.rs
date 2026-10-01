use crate::Error;
use openpilot_logmessaged::JsonValue;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

pub const MAX_AGE_SECONDS: i64 = 31 * 24 * 3600;
pub const MAX_RETRIES: i64 = 30;
pub const RETRY_DELAY_SECONDS: u64 = 10;
pub const UPLOAD_TOS: u32 = 0x20;
pub const SSH_TOS: u32 = 0x90;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UploadItem {
    pub path: String,
    pub url: String,
    pub headers: serde_json::Map<String, serde_json::Value>,
    pub created_at: i64,
    pub id: Option<String>,
    pub retry_count: i64,
    pub current: bool,
    pub progress: f64,
    pub allow_cellular: bool,
    pub priority: i64,
}

pub fn repr(value: &serde_json::Value) -> Result<String, Error> {
    let wrapped = JsonValue::parse(&format!("[{value}]"))?;
    let points = openpilot_runtime_version::python_str(&wrapped)?;
    points[1..points.len() - 1]
        .iter()
        .copied()
        .map(|point| char::from_u32(point).ok_or(Error::Contract("unencodable text")))
        .collect()
}

impl UploadItem {
    pub fn assign_id(&mut self) -> Result<(), Error> {
        let data = format!(
            "UploadItem(path={}, url={}, headers={}, created_at={}, id=None, retry_count=0, current=False, progress=0, allow_cellular={}, priority={})",
            repr(&self.path.clone().into())?, repr(&self.url.clone().into())?,
            repr(&serde_json::Value::Object(self.headers.clone()))?, self.created_at,
            if self.allow_cellular { "True" } else { "False" }, self.priority,
        );
        self.id = Some(format!("{:x}", Sha1::digest(data.as_bytes())));
        Ok(())
    }

    pub fn retry(&self, increase: bool) -> Option<Self> {
        (self.retry_count < MAX_RETRIES).then(|| Self {
            retry_count: self.retry_count + i64::from(increase),
            progress: 0.,
            current: false,
            ..self.clone()
        })
    }

    pub fn expired(&self, now_ms: i64) -> bool {
        i128::from(now_ms) - i128::from(self.created_at) > i128::from(MAX_AGE_SECONDS) * 1000
    }
}

pub fn strip_zst(path: &str) -> &str {
    path.strip_suffix(".zst").unwrap_or(path)
}

pub fn completed_status(status: u16) -> bool {
    matches!(status, 200 | 201 | 401 | 403 | 412)
}

pub fn proxy_port(port: i64) -> Result<u16, Error> {
    match port {
        22 | 8022 => Ok(22),
        _ => Err(Error::Contract("Requested local port not whitelisted")),
    }
}

pub fn backoff_limit(retries: u32) -> u32 {
    1_u32 << retries.min(7)
}

pub fn viewed_routes(current: Option<&str>, route: &str) -> String {
    let mut routes = current.map_or_else(Vec::new, |value| value.split(',').collect());
    routes.push(route);
    let mut unique = Vec::new();
    for route in routes {
        if !unique.contains(&route) {
            unique.push(route);
        }
    }
    unique[unique.len().saturating_sub(10)..].join(",")
}
