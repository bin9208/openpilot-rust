use super::*;
use base64::Engine;
use chrono::Local;

fn configured(name: &str) -> String {
    let value = std::env::var(name).unwrap_or_default();
    helpers::strip(&value).into()
}

fn endpoint() -> Result<String, Error> {
    for key in [
        "CARROT_SUPPORT_DISCORD_WEBHOOK_URL",
        "CARROT_DISCORD_WEBHOOK_URL",
        "DISCORD_WEBHOOK_URL",
    ] {
        let value = configured(key);
        if !value.is_empty() {
            return Ok(value);
        }
    }
    let encoded = "CxUGAhxOAlwRGQMMHQZJWFIMDF0THx0CBBASGAAdH15ZAFZTRkBdRxpLQ0BEWUVFFUYEUE4ZJA0lf0AmBhgINy1EJ39XAzlEOwNJHxwmJjUkGkYxVwFYQDVXDVQmEyYfHCI6AAZ0KjQZACAbRhIRHBMVGQwcBAY5CQ==";
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| Error::Source(error.to_string()))?;
    let key = b"carrot-support-v1";
    let decoded: Vec<_> = bytes
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect();
    let decoded = String::from_utf8(decoded).map_err(|error| Error::Source(error.to_string()))?;
    Ok(helpers::strip(&decoded).into())
}

fn text(value: &Value, fallback: &str) -> Result<String, Error> {
    if value.truth() {
        Ok(value.string()?)
    } else {
        Ok(fallback.into())
    }
}

pub(super) fn message(payload: &Value) -> Result<String, Error> {
    let meta = payload.get("meta");
    let note = text(payload.get("note"), "")?;
    let note = helpers::strip(&note);
    let commit = text(meta.get("commit"), "")?;
    let commit = helpers::strip(&commit);
    let commit = if commit.is_empty() || commit == "unknown" {
        "unknown".into()
    } else {
        format!("[{commit}](https://github.com/ajouatom/openpilot/commit/{commit})")
    };
    let permission = text(payload.get("permissionMode"), "approve_each")?;
    let permission = match permission.as_str() {
        "approve_each" => "항상 확인".into(),
        "allow_all" => "전체 허용".into(),
        _ => payload.get("permissionMode").string()?,
    };
    let ttl = text(payload.get("ttl_minutes"), "30")?;
    let expires = if payload.get("ttl_minutes").text_eq("unlimited") {
        "unlimited".into()
    } else {
        format!("{ttl} min")
    };
    let created = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut lines = vec![
        "# Carrot Remote Terminal (recovery)".into(),
        "### Session".into(),
        format!("- Time: {}", text(payload.get("createdAt"), &created)?),
        format!(
            "- Session ID: {}",
            text(payload.get("sessionId"), "unknown")?
        ),
        "### Access".into(),
        format!("- Link: {}", text(payload.get("url"), "")?),
        format!("- PIN: **__{}__**", text(payload.get("pin"), "")?),
        format!("- Expires: {expires}"),
        format!("- Permission: {permission}"),
    ];
    if !note.is_empty() {
        lines.push("### Issue".into());
        lines.push(format!(
            "- Note: {}",
            note.chars().take(500).collect::<String>()
        ));
    }
    lines.extend([
        "### Device".into(),
        format!("- Car name: {}", text(meta.get("carName"), "none")?),
        format!("- DongleId: {}", text(meta.get("dongleId"), "unknown")?),
        format!("- Serial: {}", text(meta.get("serial"), "unknown")?),
        format!("- Branch: {}", text(meta.get("branch"), "unknown")?),
        format!("- Commit: {commit}"),
    ]);
    Ok(lines.join("\n").chars().take(1900).collect())
}

pub(super) fn send(_config: &Config, payload: &Value) -> Result<Value, Error> {
    if matches!(
        configured("CARROT_SUPPORT_DISCORD_WEBHOOK_DISABLE")
            .to_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    ) {
        return Ok(Value::object([
            ("configured", Value::Bool(true)),
            ("ok", Value::Bool(false)),
            ("skipped", Value::Bool(true)),
            ("disabled", Value::Bool(true)),
        ]));
    }
    let url = endpoint()?;
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Ok(Value::object([
            ("configured", Value::Bool(!url.is_empty())),
            ("ok", Value::Bool(false)),
            ("skipped", Value::Bool(true)),
        ]));
    }
    let body = Value::object([
        ("username", Value::text("Carrot Support")),
        ("content", Value::text(&message(payload)?)),
        (
            "allowed_mentions",
            Value::object([("parse", Value::Array(vec![]))]),
        ),
        ("flags", Value::integer(4)),
    ])
    .encode()?;
    Ok(crate::upload::network::post_no_body(
        &url,
        &[("Content-Type", "application/json")],
        body.as_bytes(),
        Duration::from_secs(12),
    ))
}
