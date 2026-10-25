//! Python value/byte mechanics; registration state transitions live elsewhere.
use crate::{control_tools_policy_check::json, seal::Result};
use openpilot_log_format::{JsonValue, JsonView};
use std::{collections::BTreeMap, ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

pub(super) fn null() -> JsonValue {
    JsonValue::parse("null").expect("JSON null")
}
pub(super) fn text(value: &str) -> JsonValue {
    JsonValue::text(value)
}
pub(super) fn value<T: serde::Serialize>(value: T) -> Result<JsonValue> {
    Ok(JsonValue::parse(&serde_json::to_string(&value)?)?)
}
pub(super) fn object(fields: impl IntoIterator<Item = (String, JsonValue)>) -> Result<JsonValue> {
    let fields = fields
        .into_iter()
        .map(|(name, value)| Ok(format!("{}:{}", text(&name).to_json()?, value.to_json()?)))
        .collect::<Result<Vec<_>>>()?;
    Ok(JsonValue::parse(&format!("{{{}}}", fields.join(",")))?)
}
pub(super) fn array(fields: impl IntoIterator<Item = JsonValue>) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "[{}]",
        fields
            .into_iter()
            .map(|v| v.to_json())
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?)
}
pub(super) fn items(value: &JsonValue) -> Result<Vec<JsonValue>> {
    match value.view() {
        JsonView::Array(v) => Ok(v),
        _ => Err("expected array".into()),
    }
}
pub(super) fn fields(value: &JsonValue) -> Result<BTreeMap<String, JsonValue>> {
    match value.view() {
        JsonView::Object(v) => v
            .into_iter()
            .map(|(key, value)| {
                Ok((
                    key.iter()
                        .copied()
                        .map(char::from_u32)
                        .collect::<Option<String>>()
                        .ok_or("non-scalar object key")?,
                    value,
                ))
            })
            .collect(),
        _ => Err("expected object".into()),
    }
}
pub(super) fn get(value: &JsonValue, name: &str) -> Result<JsonValue> {
    value
        .get(name)
        .ok_or_else(|| format!("missing {name}").into())
}
pub(super) fn string(value: &JsonValue) -> Result<String> {
    value
        .to_utf8()
        .ok_or_else(|| "expected scalar Unicode string".into())
}
pub(super) fn required_text(value: &JsonValue, name: &str) -> Result<String> {
    string(&get(value, name)?)
}
pub(super) fn number(value: &JsonValue) -> Result<f64> {
    match value.view() {
        JsonView::Integer(v) => Ok(v.parse()?),
        JsonView::Float(v) => Ok(v),
        JsonView::Bool(v) => Ok(if v { 1.0 } else { 0.0 }),
        _ => Err("expected number".into()),
    }
}
pub(super) fn integer(value: &JsonValue) -> Result<i64> {
    match value.view() {
        JsonView::Integer(v) => Ok(v.parse()?),
        JsonView::Bool(v) => Ok(i64::from(v)),
        _ => Err("expected integer".into()),
    }
}
pub(super) fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => v != "0",
        JsonView::Float(v) => v != 0.0,
        JsonView::Text(v) => !v.is_empty(),
        JsonView::Array(v) => !v.is_empty(),
        JsonView::Object(v) => !v.is_empty(),
    }
}
pub(super) fn is_null(value: &JsonValue) -> bool {
    matches!(value.view(), JsonView::Null)
}
pub(super) fn flag(value: &JsonValue, name: &str) -> bool {
    value.get(name).is_some_and(|v| truth(&v))
}
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
pub(super) fn unhex(text: &str) -> Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return Err("odd hexadecimal byte input".into());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|v| Ok(u8::from_str_radix(std::str::from_utf8(v)?, 16)?))
        .collect()
}
pub(super) fn bytes(value: &JsonValue) -> Result<Vec<u8>> {
    unhex(&required_text(value, "bytes_hex")?)
}

pub(super) fn write(path: &std::path::Path, value: &JsonValue) -> Result<()> {
    Ok(std::fs::write(path, json::pretty(value, 0)?.as_bytes())?)
}
pub(super) fn equal(a: &JsonValue, b: &JsonValue) -> bool {
    json::equal(a, b)
}
pub(super) fn os(value: &JsonValue) -> Result<OsString> {
    let JsonView::Text(points) = value.view() else {
        return Err("filesystem path is not text".into());
    };
    let mut raw = Vec::new();
    for &point in points {
        if (0xdc80..=0xdcff).contains(&point) {
            raw.push((point - 0xdc00) as u8)
        } else {
            let mut buf = [0; 4];
            raw.extend_from_slice(
                char::from_u32(point)
                    .ok_or("invalid filesystem surrogate")?
                    .encode_utf8(&mut buf)
                    .as_bytes(),
            )
        }
    }
    Ok(OsString::from_vec(raw))
}
pub(super) fn path(value: &JsonValue, name: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(os(&get(value, name)?)?))
}
pub(super) fn path_value(value: &std::path::Path) -> JsonValue {
    crate::python_fs::json_value(value.as_os_str())
}
pub(super) fn bytes_repr(raw: &[u8]) -> String {
    let quote = if raw.contains(&b'\'') && !raw.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut out = String::from("b");
    out.push(char::from(quote));
    for &b in raw {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            b if b == quote => {
                out.push('\\');
                out.push(char::from(b))
            }
            32..=126 => out.push(char::from(b)),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push(char::from(quote));
    out
}
pub(super) fn repr(value: &JsonValue) -> String {
    match value.view() {
        JsonView::Null => "None".into(),
        JsonView::Bool(v) => if v { "True" } else { "False" }.into(),
        JsonView::Integer(v) => v.into(),
        JsonView::Float(v) => {
            if v.is_nan() {
                "nan".into()
            } else if v == f64::INFINITY {
                "inf".into()
            } else if v == f64::NEG_INFINITY {
                "-inf".into()
            } else {
                value.to_json().expect("float serialization")
            }
        }
        JsonView::Text(points) => crate::plot_samples_source::text_repr(points),
        JsonView::Array(rows) => {
            format!("[{}]", rows.iter().map(repr).collect::<Vec<_>>().join(", "))
        }
        JsonView::Object(rows) => format!(
            "{{{}}}",
            rows.iter()
                .map(|(key, value)| format!(
                    "{}: {}",
                    crate::plot_samples_source::text_repr(key),
                    repr(value)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
pub(super) fn python_str(value: &JsonValue) -> Result<Vec<u32>> {
    match value.view() {
        JsonView::Text(points) => Ok(points.to_vec()),
        _ => Ok(repr(value).chars().map(u32::from).collect()),
    }
}

pub(super) fn scalar_utf8(points: &[u32]) -> Result<String> {
    points
        .iter()
        .copied()
        .map(|point| {
            char::from_u32(point).ok_or_else(|| "UnicodeEncodeError: surrogates not allowed".into())
        })
        .collect()
}
