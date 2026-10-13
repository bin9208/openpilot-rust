use super::{
    metadata::Metadata,
    model::Session,
    policy::{self, Permission},
};
use crate::{Error, Value};
use base64::Engine;
use chrono::{Local, TimeZone};
use num_traits::ToPrimitive;

const KEY: &[u8] = b"carrot-support-v1";
const DEFAULT: &str = concat!(
    "CxUGAhxOAlwRGQMMHQZJWFIMDF0THx0CBBASGAAdH15ZAFZTRkBdRxpLQ0BEWUVFFUYEUE4ZJA0lf0Am",
    "BhgINy1EJ39XAzlEOwNJHxwmJjUkGkYxVwFYQDVXDVQmEyYfHCI6AAZ0KjQZACAbRhIRHBMVGQwcBAY5CQ=="
);
fn url() -> String {
    for name in [
        "CARROT_SUPPORT_DISCORD_WEBHOOK_URL",
        "CARROT_DISCORD_WEBHOOK_URL",
        "DISCORD_WEBHOOK_URL",
    ] {
        let value = std::env::var(name).unwrap_or_default();
        let value = value.trim_matches(crate::tools::text::whitespace);
        if !value.is_empty() {
            return value.into();
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
        .map(|value| {
            value
                .trim_matches(crate::tools::text::whitespace)
                .to_owned()
        })
        .unwrap_or_default()
}
fn message(session: &Session, metadata: &Metadata) -> Result<Value, Error> {
    let session = session.borrow();
    let created = session
        .created
        .to_i64()
        .and_then(|seconds| Local.timestamp_opt(seconds, 0).single())
        .ok_or_else(|| Error::Source("support timestamp out of range".into()))?
        .format("%Y-%m-%d %H:%M:%S");
    let permission = match session.options.permission {
        Permission::ApproveEach => "항상 확인",
        Permission::AllowAll => "전체 허용",
    };
    let commit = if metadata.commit.is_empty() || metadata.commit == "unknown" {
        "unknown".into()
    } else {
        format!(
            "[{}](https://github.com/ajouatom/openpilot/commit/{})",
            metadata.commit, metadata.commit
        )
    };
    let mut points: Vec<u32> = format!(
        "# Carrot Remote Terminal\n### Session\n- Time: {created}\n- Session ID: {}\n### Access\n- Link: {}\n- PIN: **__{}__**\n- Expires: {} min\n- Permission: {permission}",
        session.id, session.url, session.pin, (session.options.ttl / 60).max(1),
    ).chars().map(u32::from).collect();
    if session.options.permission == Permission::ApproveEach {
        points.extend(
            format!(
                "\n- Approval timeout: {} sec",
                session.options.command_timeout
            )
            .chars()
            .map(u32::from),
        );
    }
    let note = match &session.options.note {
        Value::Text(points) => crate::state::trim(points),
        _ => &[],
    };
    if !note.is_empty() {
        points.extend("\n### Issue\n- Note: ".chars().map(u32::from));
        points.extend_from_slice(note);
    }
    points.extend(format!("\n### Device\n- Car name: {}\n- DongleId: {}\n- Serial: {}\n- Branch: {}\n- Commit: {commit} ({})", metadata.car, metadata.dongle, metadata.serial, metadata.branch, metadata.date).chars().map(u32::from));
    Ok(policy::clipped(Value::Text(points), 1900))
}
fn failure(error: impl std::fmt::Display) -> Value {
    Value::object([
        ("configured", Value::Bool(true)),
        ("ok", Value::Bool(false)),
        ("error", Value::text(&error.to_string())),
    ])
}
pub(super) async fn send(
    client: &reqwest::Client,
    session: &Session,
    metadata: &Metadata,
) -> Value {
    let disabled = std::env::var("CARROT_SUPPORT_DISCORD_WEBHOOK_DISABLE").unwrap_or_default();
    if matches!(
        disabled.trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    ) {
        return Value::object([
            ("configured", Value::Bool(true)),
            ("ok", Value::Bool(false)),
            ("skipped", Value::Bool(true)),
            ("disabled", Value::Bool(true)),
        ]);
    }
    let url = url();
    if url.is_empty() {
        return Value::object([
            ("configured", Value::Bool(false)),
            ("ok", Value::Bool(false)),
            ("skipped", Value::Bool(true)),
        ]);
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return failure("invalid webhook url");
    }
    let body = message(session, metadata).and_then(|content| {
        Ok(Value::object([
            ("username", Value::text("Carrot Support")),
            ("content", content),
            (
                "allowed_mentions",
                Value::object([("parse", Value::Array(Vec::new()))]),
            ),
            ("flags", Value::integer(4)),
        ])
        .encode()?)
    });
    let body = match body {
        Ok(body) => body,
        Err(error) => return failure(error),
    };
    let operation = async {
        let response = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|error| Error::Source(error.to_string()))?;
        let status = response.status().as_u16();
        let encoding = crate::request_text::encoding(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|error| Error::Source(error.to_string()))?;
        let text = crate::request_text::decode(&bytes, &encoding)?;
        let mut fields = vec![
            ("configured", Value::Bool(true)),
            ("ok", Value::Bool((200..300).contains(&status))),
            ("status", Value::integer(status)),
        ];
        if !(200..300).contains(&status) {
            fields.push((
                "error",
                Value::text(&text.chars().take(500).collect::<String>()),
            ));
        }
        Ok::<_, Error>(Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
                .collect(),
        ))
    };
    operation.await.unwrap_or_else(failure)
}
