use crate::{diagnostic_metadata::Metadata, Error, Value};

pub(super) const FILE_LIMIT: usize = 8 * 1024 * 1024;

pub(super) fn pretty(value: &Value, depth: usize) -> Result<String, Error> {
    let padding = "  ".repeat(depth + 1);
    let rows = match value {
        Value::Text(points) => {
            return crate::state_json::utf8_text(
                &points
                    .iter()
                    .map(|point| {
                        if char::from_u32(*point).is_some() {
                            *point
                        } else {
                            u32::from('?')
                        }
                    })
                    .collect::<Vec<_>>(),
                "",
            )
        }
        Value::Array(items) if !items.is_empty() => {
            let rows = items
                .iter()
                .map(|item| Ok(format!("{padding}{}", pretty(item, depth + 1)?)))
                .collect::<Result<Vec<_>, Error>>()?;
            return Ok(format!("[\n{}\n{}]", rows.join(",\n"), "  ".repeat(depth)));
        }
        Value::Object(fields) if !fields.is_empty() => fields
            .iter()
            .map(|(key, value)| {
                Ok(format!(
                    "{padding}{}: {}",
                    pretty(&Value::Text(key.clone()), depth + 1)?,
                    pretty(value, depth + 1)?
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?,
        Value::Null
        | Value::Bool(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Array(_)
        | Value::Object(_) => return value.encode().map_err(Error::from),
    };
    Ok(format!(
        "{{\n{}\n{}}}",
        rows.join(",\n"),
        "  ".repeat(depth)
    ))
}
pub(super) fn limit(text: String) -> String {
    if text.len() <= FILE_LIMIT {
        return text;
    }
    let each = (FILE_LIMIT / 2 - 128_000).max(256_000);
    format!(
        "{}\n\n# ===== DISCORD UPLOAD TRUNCATED original_bytes={} limit={FILE_LIMIT} =====\n\n{}",
        String::from_utf8_lossy(&text.as_bytes()[..each]),
        text.len(),
        String::from_utf8_lossy(&text.as_bytes()[text.len() - each..])
    )
}
fn clean(name: &str) -> String {
    name.trim_matches(crate::tools::text::whitespace)
        .replace(['\\', '/'], "_")
}
pub(super) fn filename(name: &str) -> String {
    let name = clean(name);
    if !name.is_empty() && name.ends_with(".txt") {
        return name.chars().take(180).collect();
    }
    format!(
        "carrot_vision_diag_{}.txt",
        chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ")
    )
}
pub(super) fn console_filename(name: &str, diagnostic: &str) -> String {
    let name = clean(name);
    if !name.is_empty() && name.ends_with(".txt") {
        return name.chars().take(180).collect();
    }
    format!(
        "{}_console.txt",
        diagnostic.strip_suffix(".txt").unwrap_or(diagnostic)
    )
}
pub(super) struct Files {
    pub name: String,
    pub bytes: Vec<u8>,
    pub console_name: String,
    pub console_bytes: Vec<u8>,
}
pub(super) fn message(snapshot: &Value, metadata: &Metadata, files: &Files) -> String {
    let commit = if !metadata.commit.is_empty() && metadata.commit != "unknown" {
        format!(
            "[{}](https://github.com/bin9208/openpilot-rust/commit/{})",
            metadata.commit, metadata.commit
        )
    } else {
        "unknown".into()
    };
    let status = snapshot.get("vision_test").get("status").get("status");
    let status = if status.truth() {
        status.string().unwrap_or_else(|_| "unknown".into())
    } else {
        "unknown".into()
    };
    let streams = match snapshot.get("vipc").get("camerad_streams") {
        Value::Array(streams) if !streams.is_empty() => streams
            .iter()
            .filter_map(|stream| stream.string().ok())
            .collect::<Vec<_>>()
            .join(","),
        _ => "none".into(),
    };
    let history = match snapshot.get("stream_proxy_history") {
        Value::Array(events) => events.len(),
        _ => 0,
    };
    let port = if snapshot.get("ports").get("webrtcd_5001_open").truth() {
        "open"
    } else {
        "closed"
    };
    let mut message = format!("# Carrot Vision Diagnostic\n### Upload\n- Time: {}\n- Path: vision_diag/{} {}/\n### Device\n- Car name: {}\n- DongleId: {}\n- Serial: {}\n- Branch: {}\n- Commit: {commit} ({})\n### Result\n- Vision test: {status}\n- VIPC streams: {streams}\n- WebRTC port 5001: {port}\n- Stream requests: {history}\n- File: {} ({} bytes)",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), metadata.car, metadata.dongle, metadata.car, metadata.dongle, metadata.serial, metadata.branch, metadata.date, files.name, files.bytes.len());
    if !files.console_name.is_empty() {
        message.push_str(&format!(
            "\n- Console: {} ({} bytes)",
            files.console_name,
            files.console_bytes.len()
        ));
    }
    message.chars().take(1900).collect()
}
