use crate::{
    compatibility::{or_empty, strip, truncate},
    Error, Fields, Value,
};
use std::collections::BTreeMap;

pub const DEFAULT_WEB_UPLOAD_URL: &str = "https://upload.shind0.synology.me";
pub const DEFAULT_TMUX_WEB_UPLOAD_URL: &str = "https://tmux.carrotpilot.app/upload";

#[derive(Debug, Default, serde::Deserialize)]
pub struct Environment {
    #[serde(default)]
    pub upload_url: String,
    #[serde(default)]
    pub upload_token: String,
    #[serde(default)]
    pub tmux_url: String,
}
impl Environment {
    pub fn for_runtime() -> Result<Self, std::env::VarError> {
        fn optional(name: &str) -> Result<String, std::env::VarError> {
            match std::env::var(name) {
                Ok(value) => Ok(value),
                Err(std::env::VarError::NotPresent) => Ok(String::new()),
                Err(error) => Err(error),
            }
        }
        Ok(Self {
            upload_url: optional("CARROT_WEB_UPLOAD_URL")?,
            upload_token: optional("CARROT_WEB_UPLOAD_TOKEN")?,
            tmux_url: optional("CARROT_TMUX_WEB_UPLOAD_URL")?,
        })
    }
}
#[derive(Debug, serde::Serialize)]
pub struct Target {
    pub url: String,
    pub headers: BTreeMap<String, String>,
}

pub fn normalize_base_url(value: &str, default: &str) -> Result<String, Error> {
    let mut value = strip(value).trim_end_matches('/');
    if value.is_empty() {
        value = strip(default).trim_end_matches('/');
    }
    if !value.is_empty() && !value.starts_with("http://") && !value.starts_with("https://") {
        return Err(Error::InvalidBaseUrl);
    }
    Ok(value.into())
}
pub fn web_settings(
    settings: &Fields,
    environment: &Environment,
) -> Result<(String, String), Error> {
    let current = or_empty(settings.get("web_upload_url"))?;
    let legacy = or_empty(settings.get("toss_upload_url"))?;
    let base = [
        strip(&environment.upload_url),
        strip(&current),
        strip(&legacy),
        DEFAULT_WEB_UPLOAD_URL,
    ]
    .into_iter()
    .find(|value| !value.is_empty())
    .unwrap_or(DEFAULT_WEB_UPLOAD_URL);
    Ok((
        normalize_base_url(base, "")?,
        strip(&environment.upload_token).into(),
    ))
}
pub fn api_url(base: &str, parts: &[&str]) -> Result<String, Error> {
    let base = normalize_base_url(base, "")?;
    if base.is_empty() {
        return Err(Error::MissingBaseUrl);
    }
    let mut quoted = Vec::with_capacity(parts.len());
    for part in parts {
        let mut output = String::new();
        for byte in part.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                output.push(char::from(byte));
            } else {
                use std::fmt::Write;
                write!(output, "%{byte:02X}")?;
            }
        }
        quoted.push(output);
    }
    Ok(format!("{base}/api/v1/{}", quoted.join("/")))
}
pub fn tmux_target(
    settings: &Fields,
    environment: &Environment,
    session_token: &str,
) -> Result<Target, Error> {
    let (base, token) = web_settings(settings, environment)?;
    let token = if session_token.is_empty() {
        &token
    } else {
        session_token
    };
    let token = strip(token);
    if token.is_empty() {
        carrot_logs_target(environment)
    } else {
        Ok(Target {
            url: api_url(&base, &["tmux", "upload"])?,
            headers: BTreeMap::from([("Authorization".into(), format!("Bearer {token}"))]),
        })
    }
}
pub fn carrot_logs_target(environment: &Environment) -> Result<Target, Error> {
    Ok(Target {
        url: normalize_base_url(&environment.tmux_url, DEFAULT_TMUX_WEB_UPLOAD_URL)?,
        headers: BTreeMap::new(),
    })
}
pub fn device_id(metadata: &Fields) -> Result<String, Error> {
    for key in [
        "dongleId",
        "dongle_id",
        "deviceId",
        "device_id",
        "serial",
        "device_serial",
    ] {
        let value = or_empty(metadata.get(key))?;
        let value = strip(&value);
        if !value.is_empty() && !["unknown", "none"].contains(&value.to_lowercase().as_str()) {
            return Ok(value.into());
        }
    }
    Ok("unknown".into())
}
pub fn session_payload(metadata: &Fields, purpose: &str) -> Result<Fields, Error> {
    let mut output = metadata
        .iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                Value::Text(truncate(&or_empty(Some(value))?, 160)),
            ))
        })
        .collect::<Result<Fields, Error>>()?;
    output.insert("deviceId".into(), Value::Text(device_id(metadata)?));
    output.insert("purpose".into(), Value::Text(purpose.into()));
    Ok(output)
}
