use openpilot_logmessaged::{JsonValue, JsonView};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("navigation ingress rejected: {0}")]
pub struct IngressError(pub &'static str);

pub fn parse(frame: &[u8], strict: bool) -> Result<JsonValue, IngressError> {
    if frame.len() > super::TCP_MAX_FRAME {
        return Err(IngressError("frame_too_large"));
    }
    if frame.starts_with(b"\xef\xbb\xbf") {
        return Err(IngressError("invalid_utf8"));
    }
    let text = std::str::from_utf8(frame).map_err(|_| IngressError("invalid_utf8"))?;
    if text.trim().is_empty() {
        return Err(IngressError("empty_frame"));
    }
    let mut nesting = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in text.bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else {
            match byte {
                b'"' => in_string = true,
                b'[' | b'{' => {
                    nesting += 1;
                    if nesting > 128 {
                        return Err(IngressError("nesting_too_deep"));
                    }
                }
                b']' | b'}' => nesting = nesting.saturating_sub(1),
                _ => {}
            }
        }
    }
    let parsed = JsonValue::parse(text).map_err(|_| IngressError("invalid_json"))?;
    scan_keys(text, strict)?;
    if !parsed.is_object() {
        return Err(IngressError("non_object"));
    }
    Ok(parsed)
}

fn scan_keys(text: &str, strict: bool) -> Result<(), IngressError> {
    let mut stack: Vec<Option<BTreeSet<Vec<u32>>>> = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'{' => stack.push(Some(BTreeSet::new())),
            b'[' => stack.push(None),
            b'}' | b']' => {
                stack.pop();
            }
            b'"' => {
                let start = index;
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index += 2;
                        continue;
                    }
                    if bytes[index] == b'"' {
                        break;
                    }
                    index += 1;
                }
                let end = index + 1;
                let mut next = end;
                while bytes.get(next).is_some_and(u8::is_ascii_whitespace) {
                    next += 1;
                }
                if bytes.get(next) == Some(&b':') {
                    if let Some(Some(keys)) = stack.last_mut() {
                        let value = JsonValue::parse(&text[start..end])
                            .map_err(|_| IngressError("invalid_json"))?;
                        if let JsonView::Text(points) = value.view() {
                            if !keys.insert(points.to_vec()) {
                                return Err(IngressError("duplicate_key"));
                            }
                        }
                    }
                }
            }
            b'N' if strict && text[index..].starts_with("NaN") => {
                return Err(IngressError("non_finite_number"));
            }
            b'I' if strict && text[index..].starts_with("Infinity") => {
                return Err(IngressError("non_finite_number"));
            }
            _ => {}
        }
        index += 1;
    }
    Ok(())
}

pub fn finite_value(value: &JsonValue) -> serde_json::Value {
    match value.view() {
        JsonView::Null => serde_json::Value::Null,
        JsonView::Bool(b) => b.into(),
        JsonView::Integer(n) => serde_json::from_str(n).unwrap_or(serde_json::Value::Null),
        JsonView::Float(n) => serde_json::Number::from_f64(n).map_or_else(
            || {
                serde_json::Value::String(
                    if n.is_nan() {
                        "nan"
                    } else if n.is_sign_positive() {
                        "inf"
                    } else {
                        "-inf"
                    }
                    .into(),
                )
            },
            serde_json::Value::Number,
        ),
        JsonView::Text(points) => points
            .iter()
            .copied()
            .map(char::from_u32)
            .collect::<Option<String>>()
            .map_or(serde_json::Value::Null, serde_json::Value::String),
        JsonView::Array(values) => {
            serde_json::Value::Array(values.iter().map(finite_value).collect())
        }
        JsonView::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .filter_map(|(key, value)| {
                    key.iter()
                        .copied()
                        .map(char::from_u32)
                        .collect::<Option<String>>()
                        .map(|key| (key, finite_value(&value)))
                })
                .collect(),
        ),
    }
}
