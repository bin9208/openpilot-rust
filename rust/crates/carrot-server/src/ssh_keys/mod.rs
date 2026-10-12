//! Original server/features/ssh_keys.py and services/ssh_keys.py.
mod http;
mod online;
mod summary;
pub use http::handle;
pub use online::Online;

use crate::{param_changes::text, params::Backend, Error, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("{message}")]
    Rejected { status: u16, message: String },
    #[error(transparent)]
    Service(#[from] Error),
}

impl KeyError {
    pub fn status(&self) -> u16 {
        match self {
            Self::Rejected { status, .. } => *status,
            Self::Service(_) => 502,
        }
    }
    fn rejected(status: u16, message: impl Into<String>) -> Self {
        Self::Rejected {
            status,
            message: message.into(),
        }
    }
}

pub fn status(backend: &Backend) -> Result<Value, Error> {
    let username = backend.get("GithubUsername", &Value::text(""));
    let summaries = summary::keys(&backend.get("GithubSshKeys", &Value::text("")))?;
    let count = summaries.len();
    Ok(Value::object([
        ("username", username),
        ("has_keys", Value::Bool(count != 0)),
        ("key_count", Value::integer(count)),
        (
            "fingerprints",
            Value::Array(summaries.into_iter().take(8).collect()),
        ),
        (
            "updated_at",
            backend.get("GithubSshKeysUpdatedAt", &Value::text("")),
        ),
    ]))
}

pub fn clear(backend: &mut Backend) -> Result<Value, Error> {
    for name in ["GithubUsername", "GithubSshKeys", "GithubSshKeysUpdatedAt"] {
        if !backend.has_params() || backend.remove(name).is_err() {
            backend.put(name, &Value::text(""), None)?;
        }
    }
    status(backend)
}

pub struct Username(String);
impl Username {
    pub fn parse(value: &Value) -> Result<Self, KeyError> {
        let value = text::stripped(value, true)?;
        let Value::Text(points) = &value else {
            return Err(Error::Source("expected Python string".into()).into());
        };
        let alphanumeric =
            |point: &u32| char::from_u32(*point).is_some_and(|value| value.is_ascii_alphanumeric());
        if points.len() > 39
            || !points.first().is_some_and(alphanumeric)
            || !points.last().is_some_and(alphanumeric)
            || !points
                .iter()
                .all(|point| *point == 45 || alphanumeric(point))
        {
            return Err(KeyError::rejected(400, "invalid username"));
        }
        Ok(Self(value.string().map_err(Error::from)?))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn refresh_username(backend: &Backend) -> Result<Username, KeyError> {
    let value = backend.get("GithubUsername", &Value::text(""));
    if !text::stripped(&value, true)?.truth() {
        return Err(KeyError::rejected(400, "GitHub username is not configured"));
    }
    Username::parse(&value)
}

pub fn apply_download(
    backend: &mut Backend,
    username: &Username,
    downloaded: &str,
    timestamp: Option<i64>,
) -> Result<Value, KeyError> {
    let points: Vec<u32> = downloaded.chars().map(u32::from).collect();
    let lines = points
        .split(|point| matches!(*point, 10..=13 | 28..=30 | 133 | 8232 | 8233))
        .map(crate::state::trim)
        .filter(|line| !line.is_empty())
        .map(|line| Value::Text(line.to_vec()).string())
        .collect::<Result<Vec<_>, _>>()
        .map_err(Error::from)?;
    if lines.is_empty() {
        return Err(KeyError::rejected(
            404,
            format!("Username '{}' has no keys on GitHub", username.as_str()),
        ));
    }
    backend.put("GithubUsername", &Value::text(username.as_str()), None)?;
    backend.put("GithubSshKeys", &Value::text(&lines.join("\n")), None)?;
    let now = match timestamp {
        Some(now) => now.to_string(),
        None => timestamp_millis(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| Error::Source(error.to_string()))?,
        ),
    };
    backend.put("GithubSshKeysUpdatedAt", &Value::text(&now), None)?;
    Ok(status(backend)?)
}

fn timestamp_millis(duration: Duration) -> String {
    let nanoseconds = duration.as_nanos();
    let seconds = if nanoseconds.is_multiple_of(1_000_000_000) {
        duration.as_secs() as f64
    } else {
        nanoseconds as f64 / 1_000_000_000.0
    };
    ((seconds * 1000.0) as u128).to_string()
}

#[cfg(test)]
mod test {
    use super::timestamp_millis;
    use std::time::Duration;

    #[test]
    fn clock_milliseconds_follows_source_float_rounding() {
        let actual = timestamp_millis(Duration::from_nanos(1_700_000_000_999_999_999));
        println!("timestamp_millis={actual}");
        assert_eq!(actual, "1700000001000");
    }
}
