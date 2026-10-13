mod discord;
mod multipart;
pub mod network;

use crate::{config::Config, cwp::read_param, Error, Value};
use std::time::Duration;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default().trim().to_owned()
}

pub(super) fn string(value: &Value) -> Result<String, Error> {
    if value.truth() {
        Ok(value.string()?)
    } else {
        Ok(String::new())
    }
}

fn put(value: &mut Value, key: &str, item: Value) {
    if let Value::Object(fields) = value {
        if let Some((_, value)) = fields
            .iter_mut()
            .find(|(name, _)| name.iter().copied().eq(key.chars().map(u32::from)))
        {
            *value = item;
        } else {
            fields.push((key.chars().map(u32::from).collect(), item));
        }
    }
}

fn metadata(config: &Config, reason: &str) -> Vec<(&'static str, String)> {
    let local = crate::cwp::local_ip();
    let mut fields = vec![("tmux_why", reason.into())];
    for (name, key) in [
        ("car_name", "CarName"),
        ("git_branch", "GitBranch"),
        ("github_id", "GithubUsername"),
        ("git_remote", "GitRemote"),
        ("git_commit", "GitCommit"),
        ("git_commit_date", "GitCommitDate"),
        ("dongle_id", "DongleId"),
        ("device_serial", "HardwareSerial"),
    ] {
        fields.push((name, read_param(config, key, "")));
    }
    fields.push(("local_ip", local));
    fields
}

fn web_base(config: &Config) -> Result<String, Error> {
    let mut configured = env("CARROT_WEB_UPLOAD_URL");
    if configured.is_empty() {
        configured = std::fs::read_to_string(config.data.join("state/web_settings.json"))
            .ok()
            .and_then(|text| Value::parse(&text).ok())
            .and_then(|value| {
                string(if value.get("web_upload_url").truth() {
                    value.get("web_upload_url")
                } else {
                    value.get("toss_upload_url")
                })
                .ok()
            })
            .unwrap_or_default()
            .trim()
            .into();
    }
    if configured.is_empty() {
        configured = "https://upload.shind0.synology.me".into();
    }
    let base = configured.trim_end_matches('/');
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err(Error::Source(
            "web upload URL must start with http:// or https://".into(),
        ));
    }
    Ok(base.into())
}

fn post_file(
    url: &str,
    headers: &[(&str, &str)],
    fields: &[(&str, String)],
    raw: &[u8],
) -> Result<Value, Error> {
    let (body, content_type) = multipart::encode(fields, "tmux.log", raw)?;
    let mut headers = headers.to_vec();
    headers.push(("Content-Type", &content_type));
    Ok(network::post(url, &headers, &body, Duration::from_secs(30)))
}

fn dsm(config: &Config, fields: &[(&str, String)], raw: &[u8]) -> Result<Value, Error> {
    if let Some(webdav) =
        openpilot_web_upload::WebDav::load(&config.data.join("state/private_upload.json"))
            .map_err(|error| Error::Source(error.to_string()))?
    {
        let metadata = fields
            .iter()
            .map(|(name, value)| {
                (
                    name.to_string(),
                    openpilot_web_upload::Value::Text(value.clone()),
                )
            })
            .collect();
        let response = webdav
            .upload_tmux(&metadata, &config.tmux_log, None)
            .map_err(|error| Error::Source(error.to_string()))?;
        return Value::parse(&String::from_utf8_lossy(&response.body)).map_err(Error::from);
    }
    let base = web_base(config)?;
    let mut token = env("CARROT_WEB_UPLOAD_TOKEN");
    if token.is_empty() {
        let mut payload = Value::Object(
            fields
                .iter()
                .map(|(key, value)| {
                    (
                        key.chars().map(u32::from).collect(),
                        Value::text(&value.chars().take(160).collect::<String>()),
                    )
                })
                .collect(),
        );
        let id = fields
            .iter()
            .find(|(key, value)| *key == "dongle_id" && !value.is_empty())
            .or_else(|| {
                fields
                    .iter()
                    .find(|(key, value)| *key == "device_serial" && !value.is_empty())
            })
            .map_or("unknown", |(_, value)| value);
        put(&mut payload, "deviceId", Value::text(id));
        put(&mut payload, "purpose", Value::text("tmux"));
        let mut session = network::post(
            &format!("{base}/api/v1/session"),
            &[("Content-Type", "application/json")],
            payload.encode()?.as_bytes(),
            Duration::from_secs(12),
        );
        token = string(session.get("body").get("token"))?.trim().into();
        if !session.get("ok").truth() || !session.get("body").get("ok").truth() || token.is_empty()
        {
            let error = if session.get("error").truth() {
                session.get("error").clone()
            } else {
                Value::text("upload server did not issue a session")
            };
            put(&mut session, "ok", Value::Bool(false));
            put(&mut session, "error", error);
            return Ok(session);
        }
    }
    post_file(
        &format!("{base}/api/v1/tmux/upload"),
        &[("Authorization", &format!("Bearer {token}"))],
        fields,
        raw,
    )
}

fn carrot_logs(fields: &[(&str, String)], raw: &[u8]) -> Result<Value, Error> {
    let configured = env("CARROT_TMUX_WEB_UPLOAD_URL");
    let url = if configured.is_empty() {
        "https://tmux.carrotpilot.app/upload"
    } else {
        &configured
    }
    .trim_end_matches('/');
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(Error::Source(
            "Carrot Logs URL must start with http:// or https://".into(),
        ));
    }
    post_file(url, &[], fields, raw)
}

pub fn destinations(config: &Config, reason: &str) -> Result<Value, Error> {
    let raw = match std::fs::read(&config.tmux_log) {
        Ok(raw) => raw,
        Err(error) => {
            let detail = error.raw_os_error().map_or_else(
                || error.to_string(),
                |code| {
                    format!(
                        "[Errno {code}] {}: '{}'",
                        error
                            .to_string()
                            .split(" (os error")
                            .next()
                            .unwrap_or("I/O error"),
                        config.tmux_log.display()
                    )
                },
            );
            return Ok(Value::object([
                ("ok", Value::Bool(false)),
                (
                    "error",
                    Value::text(&format!("tmux log read failed: {detail}")),
                ),
            ]));
        }
    };
    let fields = metadata(config, reason);
    let dsm = dsm(config, &fields, &raw).unwrap_or_else(network::failure);
    let carrot = carrot_logs(&fields, &raw).unwrap_or_else(network::failure);
    let discord = discord::send(config, reason, &raw, &dsm)?;
    let results = [("dsm", dsm), ("carrot_logs", carrot), ("discord", discord)];
    let ok = results.iter().any(|(_, value)| value.get("ok").truth());
    let partial = ok && results.iter().any(|(_, value)| !value.get("ok").truth());
    let error = if ok {
        String::new()
    } else {
        results
            .iter()
            .map(|(name, value)| {
                let detail = if value.get("error").truth() {
                    value.get("error")
                } else {
                    value.get("status")
                };
                Ok(format!(
                    "{name}: {}",
                    if detail.truth() {
                        detail.string()?
                    } else {
                        "failed".into()
                    }
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?
            .join("; ")
    };
    Ok(Value::object([
        ("ok", Value::Bool(ok)),
        ("partial", Value::Bool(partial)),
        ("destinations", Value::object(results)),
        ("error", Value::text(&error)),
    ]))
}
#[cfg(test)]
mod tests;
