use crate::{report, Error};
use base64::Engine;
use openpilot_logging::producer::Logger;
use openpilot_params::Params;
use openpilot_web_upload::{Environment, Fields, Value as WireValue};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

include!(concat!(env!("OUT_DIR"), "/source_defaults.rs"));

pub(crate) fn strip(value: &str) -> &str {
    value.trim_matches(|character: char| {
        character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
    })
}
pub fn param_text(
    params: Option<&Params>,
    key: &str,
    default: &str,
    logger: &mut Logger,
) -> String {
    params
        .and_then(|params| {
            openpilot_params_typed::get_string(params, key, logger)
                .ok()
                .flatten()
        })
        .map(|value| strip(&value).to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.into())
}
pub fn git_text(repo: &Path, args: &[&str], default: &str) -> String {
    let run = || -> std::io::Result<Option<String>> {
        let mut child = Command::new("git")
            .args(args)
            .current_dir(repo)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                let output = child.wait_with_output()?;
                return Ok(status
                    .success()
                    .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
                    .filter(|s| !s.is_empty()));
            }
            if start.elapsed() >= Duration::from_secs(4) {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(5));
        }
    };
    run().ok().flatten().unwrap_or_else(|| default.into())
}
pub fn device_serial(
    params: Option<&Params>,
    environment: &BTreeMap<String, String>,
    hardware_serial: &str,
    logger: &mut Logger,
) -> String {
    for key in ["HardwareSerial", "DeviceSerial", "Serial", "CarrotSerial"] {
        let value = param_text(params, key, "", logger);
        if !value.is_empty() {
            return value;
        }
    }
    for key in ["CARROT_DEVICE_SERIAL", "DEVICE_SERIAL", "SERIAL"] {
        let value = strip(environment.get(key).map_or("", String::as_str));
        if !value.is_empty() {
            return value.into();
        }
    }
    let serial = strip(hardware_serial);
    if serial.is_empty() {
        "unknown".into()
    } else {
        serial.into()
    }
}
pub fn hardware_serial() -> String {
    if !Path::new("/TICI").is_file() {
        return String::new();
    }
    fs::read_to_string("/proc/cmdline")
        .ok()
        .and_then(|line| {
            line.split_whitespace().find_map(|pair| {
                pair.strip_prefix("androidboot.serialno=")
                    .map(str::to_owned)
            })
        })
        .unwrap_or_default()
}
pub fn upload_metadata(
    params: Option<&Params>,
    repo: &Path,
    environment: &BTreeMap<String, String>,
    serial: &str,
    logger: &mut Logger,
) -> Value {
    json!({"carName":param_text(params,"CarName","none",logger),"dongleId":param_text(params,"DongleId","unknown",logger),"serial":device_serial(params,environment,serial,logger),"branch":git_text(repo,&["branch","--show-current"],"unknown"),"commit":git_text(repo,&["rev-parse","--short","HEAD"],"unknown"),"commitDate":git_text(repo,&["show","-s","--date=format:%Y-%m-%d %H:%M:%S","--format=%cd","HEAD"],"unknown")})
}
pub fn decode_obfuscated(value: &str, key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let value = strip(value);
    if !value.is_ascii() {
        return String::new();
    }
    let padded = format!("{value}{}", "=".repeat((4 - value.len() % 4) % 4));
    let (mut clean, mut pads, mut complete) = (Vec::new(), 0usize, false);
    for byte in padded.bytes() {
        let position = clean.len() % 4;
        if byte == b'=' {
            pads += 1;
            if position >= 2 && position + pads >= 4 {
                complete = true;
                break;
            }
        } else if byte.is_ascii_alphanumeric() || b"+/-_".contains(&byte) {
            pads = 0;
            clean.push(match byte {
                b'-' => b'+',
                b'_' => b'/',
                byte => byte,
            });
        }
    }
    if !complete && clean.len() % 4 != 0 {
        return String::new();
    }
    let config = base64::engine::GeneralPurposeConfig::new()
        .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent)
        .with_decode_allow_trailing_bits(true);
    let engine = base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, config);
    let Ok(bytes) = engine.decode(clean) else {
        return String::new();
    };
    let decoded = bytes
        .iter()
        .enumerate()
        .map(|(i, byte)| byte ^ key.as_bytes()[i % key.len()])
        .collect::<Vec<_>>();
    let mut output = String::new();
    let mut remaining = decoded.as_slice();
    loop {
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                output.push_str(text);
                break;
            }
            Err(error) => {
                if let Ok(text) = std::str::from_utf8(&remaining[..error.valid_up_to()]) {
                    output.push_str(text);
                }
                let Some(length) = error.error_len() else {
                    break;
                };
                remaining = &remaining[error.valid_up_to() + length..];
            }
        }
    }
    strip(&output).into()
}
pub fn webhook_url(
    params: Option<&Params>,
    environment: &BTreeMap<String, String>,
    logger: &mut Logger,
) -> String {
    for key in ["CARROT_DISCORD_WEBHOOK_URL", "DISCORD_WEBHOOK_URL"] {
        if let Some(value) = environment
            .get(key)
            .map(|s| strip(s))
            .filter(|s| !s.is_empty())
        {
            return value.into();
        }
    }
    for key in [
        "CarrotDiscordWebhookUrl",
        "CarrotDiscordWebhookURL",
        "DiscordWebhookUrl",
        "DiscordWebhookURL",
    ] {
        let value = param_text(params, key, "", logger);
        if !value.is_empty() {
            return value;
        }
    }
    if environment
        .get("CARROT_DISCORD_WEBHOOK_DISABLE")
        .is_some_and(|value| {
            ["1", "true", "yes", "on"].contains(&strip(value).to_lowercase().as_str())
        })
    {
        return String::new();
    }
    decode_obfuscated(DEFAULT_WEBHOOK, DEFAULT_KEY)
}
pub fn fields(value: &Value) -> Result<Fields, Error> {
    serde_json::from_value::<WireValue>(value.clone())
        .map_err(Error::from)
        .and_then(|value| match value {
            WireValue::Object(fields) => Ok(fields),
            _ => Err(Error::Runtime("metadata must be an object".into())),
        })
}
pub fn target_settings(path: &Path, environment: &Environment) -> Result<(String, String), Error> {
    let raw: Value = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);
    let value = raw
        .get("web_upload_url")
        .or_else(|| raw.get("toss_upload_url"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let normalized = openpilot_web_upload::normalize_base_url(
        value,
        openpilot_web_upload::DEFAULT_WEB_UPLOAD_URL,
    )
    .unwrap_or_else(|_| openpilot_web_upload::DEFAULT_WEB_UPLOAD_URL.into());
    let normalized = if ["https://op.wjcloud.kr", "https://shind0.synology.me"]
        .contains(&normalized.to_lowercase().as_str())
    {
        openpilot_web_upload::DEFAULT_WEB_UPLOAD_URL.to_owned()
    } else {
        normalized
    };
    Ok(openpilot_web_upload::web_settings(
        &fields(&json!({"web_upload_url":normalized}))?,
        environment,
    )?)
}
pub fn send_webhook(url: &str, payload: &Value) -> Value {
    let url = strip(url);
    if url.is_empty() {
        return json!({"configured":false,"ok":false,"skipped":true});
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return json!({"configured":true,"ok":false,"error":"invalid webhook url"});
    }
    let body = json!({"username":"Carrot Dashcam","content":report::discord_content(payload),"allowed_mentions":{"parse":[]},"flags":4});
    let request = fields(&body)
        .and_then(|body| openpilot_web_upload::post_json_total(url, &body).map_err(Error::from))
        .and_then(|response| {
            let text = response.text()?;
            Ok((response.status, text))
        });
    match request {
        Ok((status, _)) if (200..300).contains(&status) => {
            json!({"configured":true,"ok":true,"status":status})
        }
        Ok((status, text)) => {
            json!({"configured":true,"ok":false,"status":status,"error":text.chars().take(500).collect::<String>()})
        }
        Err(error) => json!({"configured":true,"ok":false,"error":error.to_string()}),
    }
}
pub fn runtime_paths() -> (PathBuf, PathBuf) {
    let repo = env::var_os("CARROT_REPO_DIR")
        .map_or_else(|| PathBuf::from("/data/openpilot"), PathBuf::from);
    let data =
        env::var_os("CARROT_DATA_DIR").map_or_else(|| PathBuf::from("/data/carrot"), PathBuf::from);
    (repo, data.join("state/web_settings.json"))
}
