use super::Value;
use base64::Engine;

pub(crate) fn handshake_error(headers: &hyper::HeaderMap) -> Option<String> {
    let header = |name| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
    };
    if !header("upgrade").trim().eq_ignore_ascii_case("websocket") {
        return Some(format!(
            "No WebSocket UPGRADE hdr: {}\n Can \"Upgrade\" only to \"WebSocket\".",
            headers
                .get("upgrade")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("None")
        ));
    }
    if !header("connection").to_lowercase().contains("upgrade") {
        return Some(format!(
            "No CONNECTION upgrade hdr: {}",
            headers
                .get("connection")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("None")
        ));
    }
    if !matches!(header("sec-websocket-version"), "13" | "8" | "7") {
        return Some(format!(
            "Unsupported version: {}",
            header("sec-websocket-version")
        ));
    }
    let key = header("sec-websocket-key");
    let filtered: Vec<_> = key
        .bytes()
        .filter(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
        .collect();
    if base64::engine::general_purpose::STANDARD
        .decode(&filtered)
        .map_or(true, |decoded| decoded.len() != 16)
    {
        let key = if headers.contains_key("sec-websocket-key") {
            Value::text(key).repr().unwrap_or_else(|_| "None".into())
        } else {
            "None".into()
        };
        return Some(format!("Handshake error: {key}"));
    }
    None
}
