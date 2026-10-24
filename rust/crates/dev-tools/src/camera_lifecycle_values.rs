//! Python value/error operations used only by the original camera QA closure.
use crate::{control_tools_policy_check::json, seal::Result};
use openpilot_log_format::{JsonValue, JsonView};
use std::fmt;
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
pub fn error(kind: &'static str, message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(PythonError {
        kind,
        message: message.into(),
    })
}
pub fn assert(condition: bool, message: impl Into<String>) -> Result<()> {
    if !condition {
        return Err(error("AssertionError", message));
    }
    Ok(())
}
pub fn null() -> JsonValue {
    JsonValue::parse("null").unwrap()
}
pub fn number(value: impl ToString) -> JsonValue {
    JsonValue::parse(&value.to_string()).unwrap()
}
pub fn array(values: &[JsonValue]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "[{}]",
        values
            .iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(", ")
    ))?)
}
pub fn object(fields: &[(String, JsonValue)]) -> Result<JsonValue> {
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
pub fn typename(value: &JsonValue) -> &'static str {
    match value.view() {
        JsonView::Null => "NoneType",
        JsonView::Bool(_) => "bool",
        JsonView::Integer(_) => "int",
        JsonView::Float(_) => "float",
        JsonView::Text(_) => "str",
        JsonView::Array(_) => "list",
        JsonView::Object(_) => "dict",
    }
}
pub fn get(value: &JsonValue, key: &str) -> Result<JsonValue> {
    match value.view() {
        JsonView::Object(_) => value
            .get(key)
            .ok_or_else(|| error("KeyError", repr(&JsonValue::text(key)).unwrap())),
        JsonView::Array(_) => Err(error(
            "TypeError",
            "list indices must be integers or slices, not str",
        )),
        JsonView::Text(_) => Err(error(
            "TypeError",
            "string indices must be integers, not 'str'",
        )),
        _ => Err(error(
            "TypeError",
            format!("'{}' object is not subscriptable", typename(value)),
        )),
    }
}
pub fn optional(value: &JsonValue, key: &str) -> Result<JsonValue> {
    if !matches!(value.view(), JsonView::Object(_)) {
        return Err(error(
            "AttributeError",
            format!("'{}' object has no attribute 'get'", typename(value)),
        ));
    }
    Ok(value.get(key).unwrap_or_else(null))
}
pub fn set(value: &JsonValue, key: &str, item: JsonValue) -> Result<JsonValue> {
    let JsonView::Object(fields) = value.view() else {
        return Err(error(
            "TypeError",
            format!(
                "'{}' object does not support item assignment",
                typename(value)
            ),
        ));
    };
    let selected = key.chars().map(u32::from).collect::<Vec<_>>();
    let mut output = vec![];
    let mut replaced = false;
    for (name, value) in fields {
        let name_json = JsonValue::codepoints(name.to_vec())
            .ok_or("invalid key")?
            .to_json()?;
        let value = if name == selected {
            replaced = true;
            item.clone()
        } else {
            value
        };
        output.push(format!("{name_json}: {}", value.to_json()?));
    }
    if !replaced {
        output.push(format!(
            "{}: {}",
            JsonValue::text(key).to_json()?,
            item.to_json()?
        ));
    }
    Ok(JsonValue::parse(&format!("{{{}}}", output.join(", ")))?)
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
pub fn repr(value: &JsonValue) -> Result<String> {
    let wrapped = array(std::slice::from_ref(value))?;
    let p = openpilot_runtime_version::python_str(&wrapped)?;
    Ok(p[1..p.len() - 1]
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()
        .ok_or("surrogate in diagnostic")?)
}
pub fn sequence(value: &JsonValue) -> Result<Vec<JsonValue>> {
    json::sequence(value).map_err(|_| {
        error(
            "TypeError",
            format!("'{}' object is not iterable", typename(value)),
        )
    })
}
pub fn hashable(value: &JsonValue) -> Result<()> {
    if matches!(value.view(), JsonView::Array(_) | JsonView::Object(_)) {
        return Err(error(
            "TypeError",
            format!("unhashable type: '{}'", typename(value)),
        ));
    }
    Ok(())
}
pub fn hex(value: &JsonValue) -> Result<Vec<u8>> {
    let JsonView::Text(points) = value.view() else {
        return Err(error(
            "TypeError",
            format!("fromhex() argument must be str, not {}", typename(value)),
        ));
    };
    let digit = |v: u32| {
        char::from_u32(v).and_then(|c| {
            if c.is_ascii_hexdigit() {
                c.to_digit(16)
            } else {
                None
            }
        })
    };
    let bad = |at| {
        error(
            "ValueError",
            format!("non-hexadecimal number found in fromhex() arg at position {at}"),
        )
    };
    let mut index = 0;
    let mut out = vec![];
    while index < points.len() {
        if matches!(points[index], 9..=13 | 32) {
            index += 1;
            continue;
        }
        let high = digit(points[index]).ok_or_else(|| bad(index))?;
        let low = points
            .get(index + 1)
            .and_then(|&v| digit(v))
            .ok_or_else(|| {
                bad(if index + 1 == points.len() {
                    points.len()
                } else {
                    index + 1
                })
            })?;
        out.push((high * 16 + low) as u8);
        index += 2;
    }
    Ok(out)
}
pub fn hex_text(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn word_bytes(bytes: &[u8], offset: usize) -> u32 {
    bytes
        .iter()
        .skip(offset)
        .take(4)
        .enumerate()
        .fold(0, |v, (i, b)| v | (u32::from(*b) << (i * 8)))
}
pub fn word(row: &JsonValue, field: &str, offset: usize) -> Result<u32> {
    Ok(word_bytes(&hex(&get(row, field)?)?, offset))
}

// json.loads returns a shared NaN constant. Hash containers test identity before
// scalar equality, so repeated decoded NaN descriptor keys identify one entry.
pub fn key_equal(a: &JsonValue, b: &JsonValue) -> bool {
    matches!((a.view(),b.view()),(JsonView::Float(a),JsonView::Float(b)) if a.is_nan()&&b.is_nan())
        || json::equal(a, b)
}
