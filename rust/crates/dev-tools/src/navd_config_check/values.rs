//! Lossless request/Unix-path mechanics, with no policy expectations.
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    ffi::OsString,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
};
pub(super) fn value(v: impl serde::Serialize) -> Result<JsonValue> {
    Ok(JsonValue::parse(&serde_json::to_string(&v)?)?)
}
pub(super) fn object(fields: impl IntoIterator<Item = (String, JsonValue)>) -> Result<JsonValue> {
    let fields = fields
        .into_iter()
        .map(|(k, v)| {
            Ok(format!(
                "{}:{}",
                JsonValue::text(&k).to_json()?,
                v.to_json()?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(JsonValue::parse(&format!("{{{}}}", fields.join(",")))?)
}
pub(super) fn field(v: &JsonValue, k: &str) -> Result<JsonValue> {
    v.get(k).ok_or_else(|| format!("missing {k}").into())
}
pub(super) fn text(v: &JsonValue) -> Result<String> {
    v.to_utf8()
        .ok_or_else(|| "expected Unicode scalar string".into())
}
pub(super) fn fields(v: &JsonValue) -> Result<Vec<(String, JsonValue)>> {
    match v.view() {
        JsonView::Object(rows) => rows
            .into_iter()
            .map(|(key, v)| {
                Ok((
                    key.iter()
                        .map(|&p| char::from_u32(p))
                        .collect::<Option<String>>()
                        .ok_or("invalid key")?,
                    v,
                ))
            })
            .collect(),
        _ => Err("expected request object".into()),
    }
}
pub(super) fn flag(v: &JsonValue, k: &str) -> bool {
    v.get(k)
        .is_some_and(|v| matches!(v.view(), JsonView::Bool(true)))
}
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub(super) fn unhex(s: &str) -> Result<Vec<u8>> {
    if !s.is_ascii() || !s.len().is_multiple_of(2) {
        return Err("invalid byte hex".into());
    }
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
pub(super) fn bytes(v: &JsonValue) -> Result<Vec<u8>> {
    unhex(&text(&field(v, "bytes_hex")?)?)
}
pub(super) fn path(v: &JsonValue) -> Result<PathBuf> {
    match v.view() {
        JsonView::Text(points) => {
            let mut b = Vec::new();
            for &p in points {
                if (0xdc80..=0xdcff).contains(&p) {
                    b.push((p - 0xdc00) as u8)
                } else {
                    let c = char::from_u32(p).ok_or("invalid filesystem surrogate")?;
                    b.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes())
                }
            }
            Ok(OsString::from_vec(b).into())
        }
        _ => Err("expected path string".into()),
    }
}
pub(super) fn write(path: &Path, value: &JsonValue) -> Result<()> {
    std::fs::write(
        path,
        format!(
            "{}\n",
            crate::control_tools_policy_check::json::pretty(value, 0)?
        ),
    )?;
    Ok(())
}
pub(super) fn equal(a: &JsonValue, b: &JsonValue) -> bool {
    crate::control_tools_policy_check::json::equal(a, b)
}

pub(super) fn raw_fields(v: &JsonValue) -> Result<Vec<(JsonValue, JsonValue)>> {
    match v.view() {
        JsonView::Object(rows) => rows
            .into_iter()
            .map(|(key, value)| {
                Ok((
                    JsonValue::codepoints(key.to_vec()).ok_or("invalid JSON field")?,
                    value,
                ))
            })
            .collect(),
        _ => Err("expected object".into()),
    }
}
pub(super) fn raw_object(
    fields: impl IntoIterator<Item = (JsonValue, JsonValue)>,
) -> Result<JsonValue> {
    let fields = fields
        .into_iter()
        .map(|(key, value)| Ok(format!("{}:{}", key.to_json()?, value.to_json()?)))
        .collect::<Result<Vec<_>>>()?;
    Ok(JsonValue::parse(&format!("{{{}}}", fields.join(",")))?)
}
