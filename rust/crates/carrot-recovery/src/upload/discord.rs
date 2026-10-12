use super::{env, multipart, network};
use crate::{config::Config, cwp::read_param, Error, Value};
use base64::Engine;
use chrono::Local;
use std::time::Duration;

const KEY: &[u8] = b"carrot-exception-v1";
const DEFAULT: &str = concat!(
    "CxUGAhxOAkocChYTGxsLQE4ZXEwAAhtAA0gHEAwKGwdGXlsfQgdTUEVKXkEcUkpaVEdEWkAkRwMCFzEL",
    "MF8zS1YzWh8YJlkpVk4EUwQ0ED02IkgXKjMkQzIYIRt/HgUWUTUQWCcaAS1XKhpFUT4cGDBnLiACOx1DXQ=="
);

fn url(config: &Config) -> String {
    if matches!(
        env("CARROT_EXCEPTION_DISCORD_WEBHOOK_DISABLE")
            .to_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    ) {
        return String::new();
    }
    for name in [
        "CARROT_EXCEPTION_DISCORD_WEBHOOK_URL",
        "CARROT_DISCORD_WEBHOOK_URL",
        "DISCORD_WEBHOOK_URL",
    ] {
        let value = env(name);
        if !value.is_empty() {
            return value;
        }
    }
    for name in [
        "CarrotExceptionDiscordWebhookUrl",
        "CarrotDiscordWebhookUrl",
        "CarrotDiscordWebhookURL",
        "DiscordWebhookUrl",
        "DiscordWebhookURL",
    ] {
        let value = read_param(config, name, "");
        if !value.is_empty() {
            return value;
        }
    }
    base64::engine::general_purpose::STANDARD
        .decode(DEFAULT)
        .ok()
        .and_then(|bytes| {
            String::from_utf8(
                bytes
                    .into_iter()
                    .enumerate()
                    .map(|(index, byte)| byte ^ KEY[index % KEY.len()])
                    .collect(),
            )
            .ok()
        })
        .unwrap_or_default()
        .trim()
        .into()
}

fn repository(config: &Config) -> String {
    let mut remote = read_param(config, "GitRemote", "");
    if let Some(path) = remote.strip_prefix("git@github.com:") {
        remote = format!("https://github.com/{path}");
    }
    if remote.starts_with("https://github.com/") || remote.starts_with("http://github.com/") {
        return remote.strip_suffix(".git").unwrap_or(&remote).replacen(
            "http://github.com/",
            "https://github.com/",
            1,
        );
    }
    let user = read_param(config, "GithubUsername", "");
    format!(
        "https://github.com/{}/openpilot",
        if user.is_empty() { "ajouatom" } else { &user }
    )
}

fn content(config: &Config, reason: &str, result: &Value) -> Result<String, Error> {
    let repo = repository(config);
    let commit = read_param(config, "GitCommit", "unknown");
    let commit_text = if commit.is_empty() || commit == "unknown" {
        "unknown".into()
    } else {
        format!(
            "[{}]({repo}/commit/{commit})",
            commit.chars().take(8).collect::<String>()
        )
    };
    let mut web = if result.get("ok").truth() {
        "ok".to_owned()
    } else {
        "failed".into()
    };
    if result.get("status") != &Value::Null {
        web.push_str(&format!(" ({})", result.get("status").string()?));
    }
    Ok(format!("# Carrot Exception\n### Upload\n- Time: {}\n- Reason: {reason}\n- Web: {web}\n### Device\n- Car name: {}\n- DongleId: {}\n- Serial: {}\n- GitHub: {repo}\n- Branch: {}\n- Commit: {commit_text} ({})",
        Local::now().format("%Y-%m-%d %H:%M:%S"),read_param(config,"CarName","none"),read_param(config,"DongleId","unknown"),read_param(config,"HardwareSerial","unknown"),read_param(config,"GitBranch","unknown"),read_param(config,"GitCommitDate","unknown")
    ).chars().take(1900).collect())
}

pub(super) fn send(config: &Config, reason: &str, raw: &[u8], web: &Value) -> Result<Value, Error> {
    let address = url(config);
    if !address.starts_with("http://") && !address.starts_with("https://") {
        return Ok(Value::object([
            ("configured", Value::Bool(!address.is_empty())),
            ("ok", Value::Bool(false)),
            ("error", Value::text("Discord webhook is not configured")),
        ]));
    }
    let branch = read_param(config, "GitBranch", "unknown").replace(['/', '\\'], "__");
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let mut filename = format!("{reason}-{stamp}-{branch}.txt");
    let clipped;
    let bytes = if raw.len() > 8 * 1024 * 1024 {
        let marker = b"\n\n===== DISCORD TMUX TRUNCATED =====\n\n";
        let keep = (8 * 1024 * 1024 - marker.len()) / 2;
        clipped = [&raw[..keep], marker.as_slice(), &raw[raw.len() - keep..]].concat();
        filename = format!("{reason}-{stamp}-{branch}-truncated.txt");
        &clipped
    } else {
        raw
    };
    let payload = Value::object([
        ("username", Value::text("Carrot Exception")),
        ("content", Value::text(&content(config, reason, web)?)),
        (
            "allowed_mentions",
            Value::object([("parse", Value::Array(Vec::new()))]),
        ),
        ("flags", Value::integer(4)),
    ]);
    let (body, mime) = multipart::encode(&[("payload_json", payload.encode()?)], &filename, bytes)?;
    let mut result = network::post(
        &address,
        &[("Content-Type", &mime)],
        &body,
        Duration::from_secs(12),
    );
    if let Value::Object(fields) = &mut result {
        fields.retain(|(name, _)| !name.iter().copied().eq("body".chars().map(u32::from)));
    }
    Ok(result)
}
