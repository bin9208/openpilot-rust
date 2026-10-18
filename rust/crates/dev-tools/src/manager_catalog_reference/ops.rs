//! Generic Python JSON operations; no manager or Params policy is imported.
use crate::{control_tools_policy_check::json, seal::Result};
use openpilot_log_format::{JsonValue, JsonView};
use std::fmt;
use std::{ffi::OsString, os::unix::ffi::OsStringExt};
#[derive(Debug)]
pub struct PythonError {
    pub kind: &'static str,
    pub message: String,
}
impl fmt::Display for PythonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for PythonError {}
pub fn null() -> JsonValue {
    JsonValue::parse("null").unwrap()
}
pub fn number(value: impl ToString) -> JsonValue {
    JsonValue::parse(&value.to_string()).unwrap()
}
pub fn get(value: &JsonValue, key: &str) -> Result<JsonValue> {
    if !matches!(value.view(), JsonView::Object(_)) {
        return Err(error("TypeError", "object is not subscriptable by str"));
    }
    value.get(key).ok_or_else(|| error("KeyError", key))
}
pub fn optional(value: &JsonValue, key: &str) -> Result<JsonValue> {
    if !matches!(value.view(), JsonView::Object(_)) {
        return Err(error("AttributeError", "object has no attribute get"));
    }
    Ok(value.get(key).unwrap_or_else(null))
}
pub fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => v.trim_start_matches('-').bytes().any(|b| b != b'0'),
        JsonView::Float(v) => v != 0.0,
        JsonView::Text(v) => !v.is_empty(),
        JsonView::Array(v) => !v.is_empty(),
        JsonView::Object(v) => !v.is_empty(),
    }
}
pub fn sequence(value: &JsonValue) -> Result<Vec<JsonValue>> {
    json::sequence(value).map_err(|_| error("TypeError", "object is not iterable"))
}
pub fn object_dynamic(fields: &[(String, JsonValue)]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "{{{}}}",
        fields
            .iter()
            .map(|(key, value)| Ok(format!(
                "{}: {}",
                JsonValue::text(key).to_json()?,
                value.to_json()?
            )))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))?)
}
pub fn error(kind: &'static str, detail: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(PythonError {
        kind,
        message: detail.into(),
    })
}
pub fn boolean(v: bool) -> JsonValue {
    JsonValue::parse(if v { "true" } else { "false" }).unwrap()
}
pub fn text(v: &str) -> JsonValue {
    JsonValue::text(v)
}
pub fn optional_text(v: Option<&str>) -> JsonValue {
    v.map(text).unwrap_or_else(null)
}
pub fn array(v: &[JsonValue]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "[{}]",
        v.iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(", ")
    ))?)
}
pub fn object(v: &[(&str, JsonValue)]) -> Result<JsonValue> {
    object_dynamic(
        &v.iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<Vec<_>>(),
    )
}
pub fn default(v: &JsonValue, key: &str, fallback: JsonValue) -> Result<JsonValue> {
    optional(v, key)?;
    Ok(v.get(key).unwrap_or(fallback))
}
pub fn os_text(v: &JsonValue) -> Result<OsString> {
    let JsonView::Text(points) = v.view() else {
        return Err(error("TypeError", "expected str"));
    };
    let mut bytes = Vec::new();
    for &p in points {
        let c = char::from_u32(p)
            .ok_or_else(|| error("UnicodeEncodeError", "surrogates not allowed"))?;
        bytes.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
    }
    Ok(OsString::from_vec(bytes))
}
pub fn utf8(v: &JsonValue) -> Result<String> {
    os_text(v)?
        .into_string()
        .map_err(|_| error("UnicodeEncodeError", "invalid text"))
}
pub fn bytes(v: &JsonValue) -> Result<Vec<u8>> {
    if let JsonView::Integer(n) = v.view() {
        let n = n.parse::<isize>().map_err(|_| {
            error(
                "OverflowError",
                "cannot fit int into an index-sized integer",
            )
        })?;
        if n < 0 {
            return Err(error("ValueError", "negative count"));
        }
        let n = n as usize;
        let mut out = Vec::new();
        out.try_reserve_exact(n)
            .map_err(|_| error("MemoryError", "bytes allocation"))?;
        out.resize(n, 0);
        return Ok(out);
    }
    if let JsonView::Bool(v) = v.view() {
        return Ok(vec![0; usize::from(v)]);
    }
    if matches!(v.view(), JsonView::Text(_)) {
        return Err(error("TypeError", "string argument without an encoding"));
    }
    let mut out = Vec::new();
    for item in sequence(v)? {
        let n = match item.view() {
            JsonView::Bool(v) => i128::from(v),
            JsonView::Integer(n) => n
                .parse::<i128>()
                .map_err(|_| error("ValueError", "bytes must be in range(0, 256)"))?,
            _ => {
                return Err(error(
                    "TypeError",
                    "object cannot be interpreted as integer",
                ))
            }
        };
        out.push(
            u8::try_from(n).map_err(|_| error("ValueError", "bytes must be in range(0, 256)"))?,
        );
    }
    Ok(out)
}
pub fn classify_io(e: std::io::Error) -> Box<dyn std::error::Error> {
    let kind = match e.raw_os_error() {
        Some(21) => "IsADirectoryError",
        Some(20) => "NotADirectoryError",
        Some(2) => "FileNotFoundError",
        Some(17) => "FileExistsError",
        Some(13 | 1) => "PermissionError",
        _ => "OSError",
    };
    error(kind, e.to_string())
}
