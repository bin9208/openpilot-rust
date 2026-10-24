//! Lossless Python JSON/string mechanics; no version policy or candidate imports.
use super::{Error, Result};
use openpilot_log_format::{JsonValue, JsonView};
use std::{ffi::OsString, os::unix::ffi::OsStringExt};

pub(super) fn null() -> JsonValue {
    JsonValue::parse("null").unwrap()
}
pub(super) fn boolean(value: bool) -> JsonValue {
    JsonValue::parse(if value { "true" } else { "false" }).unwrap()
}
// The generic JSON parser accepts raw surrogate codepoints separately from
// escaped pairs. Assemble through that boundary rather than stringify/reparse,
// which would combine two independently stored adjacent lone surrogates.
fn json_points(value: &JsonValue) -> Vec<u32> {
    enum Task {
        Node(JsonValue),
        Text(Vec<u32>),
        Token(&'static str),
    }
    let mut stack = vec![Task::Node(value.clone())];
    let mut out = Vec::new();
    while let Some(task) = stack.pop() {
        let node = match task {
            Task::Token(token) => {
                out.extend(token.chars().map(u32::from));
                continue;
            }
            Task::Text(points) => {
                out.push(34);
                for point in points {
                    match point {
                        34 | 92 => {
                            out.push(92);
                            out.push(point);
                        }
                        0..=31 => out.extend(format!("\\u{point:04x}").chars().map(u32::from)),
                        _ => out.push(point),
                    }
                }
                out.push(34);
                continue;
            }
            Task::Node(node) => node,
        };
        match node.view() {
            JsonView::Text(points) => stack.push(Task::Text(points.to_vec())),
            JsonView::Array(values) => {
                out.push(91);
                stack.push(Task::Token("]"));
                for (index, value) in values.into_iter().enumerate().rev() {
                    stack.push(Task::Node(value));
                    if index > 0 {
                        stack.push(Task::Token(","));
                    }
                }
            }
            JsonView::Object(fields) => {
                out.push(123);
                stack.push(Task::Token("}"));
                for (index, (key, value)) in fields.into_iter().enumerate().rev() {
                    stack.push(Task::Node(value));
                    stack.push(Task::Token(":"));
                    stack.push(Task::Text(key.to_vec()));
                    if index > 0 {
                        stack.push(Task::Token(","));
                    }
                }
            }
            _ => out.extend(node.to_json().unwrap().chars().map(u32::from)),
        }
    }
    out
}
pub(super) fn object(fields: &[(&str, JsonValue)]) -> JsonValue {
    let mut points = vec![123];
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            points.push(44);
        }
        points.extend(json_points(&JsonValue::text(key)));
        points.push(58);
        points.extend(json_points(value));
    }
    points.push(125);
    JsonValue::parse_codepoints(&points).expect("lossless JSON object")
}
pub(super) fn array(values: &[JsonValue]) -> JsonValue {
    let mut points = vec![91];
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            points.push(44);
        }
        points.extend(json_points(value));
    }
    points.push(93);
    JsonValue::parse_codepoints(&points).expect("lossless JSON array")
}
pub(super) fn required(value: &JsonValue, key: &str) -> Result<JsonValue> {
    if !value.is_object() {
        return Err(Error::new(
            "TypeError",
            "object is not subscriptable by str",
        ));
    }
    value.get(key).ok_or_else(|| Error::new("KeyError", key))
}
pub(super) fn default(value: &JsonValue, key: &str, fallback: JsonValue) -> Result<JsonValue> {
    if !value.is_object() {
        return Err(Error::new("AttributeError", "object has no attribute get"));
    }
    Ok(value.get(key).unwrap_or(fallback))
}
pub(super) fn points(value: &JsonValue, kind: &'static str) -> Result<Vec<u32>> {
    match value.view() {
        JsonView::Text(points) => Ok(points.to_vec()),
        _ => Err(Error::new(kind, "expected str")),
    }
}
pub(super) fn text(points: Vec<u32>) -> JsonValue {
    JsonValue::codepoints(points).expect("valid Python text")
}
pub(super) fn os_string(value: &JsonValue) -> Result<OsString> {
    let mut bytes = Vec::new();
    for point in points(value, "TypeError")? {
        if (0xdc80..=0xdcff).contains(&point) {
            bytes.push((point - 0xdc00) as u8);
        } else {
            let character = char::from_u32(point)
                .ok_or_else(|| Error::new("UnicodeEncodeError", "surrogates not allowed"))?;
            bytes.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
        }
    }
    Ok(OsString::from_vec(bytes))
}
pub(super) fn parse(points: &[u32]) -> Result<JsonValue> {
    JsonValue::parse_codepoints(points).map_err(|error| {
        let kind = match &error {
            openpilot_log_format::JsonError::IntegerLimit => "ValueError",
            _ => "JSONDecodeError",
        };
        Error::new(kind, error.to_string())
    })
}
pub(super) fn capture(value: Result<JsonValue>) -> JsonValue {
    match value {
        Ok(value) => object(&[("value", value)]),
        Err(error) => object(&[("error", JsonValue::text(error.kind()))]),
    }
}
pub(super) fn normalized_origin(value: &JsonValue) -> Result<JsonValue> {
    let mut source = points(value, "AttributeError")?;
    for (from, to) in [("git@", ""), (".git", ""), ("https://", ""), (":", "/")] {
        let needle: Vec<u32> = from.chars().map(u32::from).collect();
        if let Some(index) = source.windows(needle.len()).position(|part| part == needle) {
            source.splice(index..index + needle.len(), to.chars().map(u32::from));
        }
    }
    Ok(text(source))
}
pub(super) fn short_version(value: &JsonValue) -> Result<JsonValue> {
    let source = points(value, "AttributeError")?;
    let end = source
        .iter()
        .position(|&point| point == 45)
        .unwrap_or(source.len());
    Ok(text(source[..end].to_vec()))
}
pub(super) fn slice_six(value: &JsonValue) -> Result<JsonValue> {
    match value.view() {
        JsonView::Text(points) => Ok(text(points[..points.len().min(6)].to_vec())),
        JsonView::Array(values) => Ok(array(&values[..values.len().min(6)])),
        JsonView::Object(_) => Err(Error::new("KeyError", "slice(None, 6, None)")),
        _ => Err(Error::new("TypeError", "object is not subscriptable")),
    }
}

/// Python str() and container repr() assembled iteratively; nested text uses the
/// existing shared Unicode printability mechanics, never a version answer table.
pub(super) fn string(value: &JsonValue) -> Vec<u32> {
    enum Task {
        Node(JsonValue, bool),
        Token(String),
    }
    let mut tasks = vec![Task::Node(value.clone(), true)];
    let mut out = Vec::new();
    while let Some(task) = tasks.pop() {
        let (node, top) = match task {
            Task::Token(token) => {
                out.extend(token.chars().map(u32::from));
                continue;
            }
            Task::Node(node, top) => (node, top),
        };
        match node.view() {
            JsonView::Text(points) => {
                if top {
                    out.extend_from_slice(points);
                } else {
                    out.extend(
                        crate::plot_samples_source::text_repr(points)
                            .chars()
                            .map(u32::from),
                    );
                }
            }
            JsonView::Null => out.extend("None".chars().map(u32::from)),
            JsonView::Bool(value) => {
                out.extend(if value { "True" } else { "False" }.chars().map(u32::from))
            }
            JsonView::Integer(value) => out.extend(value.chars().map(u32::from)),
            JsonView::Float(value) => {
                let token = if value.is_nan() {
                    "nan".into()
                } else if value == f64::INFINITY {
                    "inf".into()
                } else if value == f64::NEG_INFINITY {
                    "-inf".into()
                } else {
                    node.to_json().unwrap()
                };
                out.extend(token.chars().map(u32::from));
            }
            JsonView::Array(values) => {
                out.push(91);
                tasks.push(Task::Token("]".into()));
                for (index, value) in values.into_iter().enumerate().rev() {
                    tasks.push(Task::Node(value, false));
                    if index > 0 {
                        tasks.push(Task::Token(", ".into()));
                    }
                }
            }
            JsonView::Object(values) => {
                out.push(123);
                tasks.push(Task::Token("}".into()));
                for (index, (key, value)) in values.into_iter().enumerate().rev() {
                    tasks.push(Task::Node(value, false));
                    tasks.push(Task::Token(": ".into()));
                    tasks.push(Task::Token(crate::plot_samples_source::text_repr(key)));
                    if index > 0 {
                        tasks.push(Task::Token(", ".into()));
                    }
                }
            }
        }
    }
    out
}
pub(super) fn format_parts(parts: &[JsonValue], separator: &str) -> JsonValue {
    let mut out = Vec::new();
    for (index, value) in parts.iter().enumerate() {
        if index > 0 {
            out.extend(separator.chars().map(u32::from));
        }
        out.extend(string(value));
    }
    text(out)
}

/// functools._make_key keeps single exact ints/strings distinct from argument
/// tuples, retains call arity and never resolves lexical filesystem strings.
pub(super) fn cache_key(args: &[JsonValue]) -> Result<String> {
    if args.len() == 1 {
        match args[0].view() {
            JsonView::Text(points) => return Ok(format!("fast-str:{points:?}")),
            JsonView::Integer(value) => return Ok(format!("fast-int:{value}")),
            _ => (),
        }
    }
    let mut keys = Vec::new();
    for value in args {
        keys.push(match value.view() {
            JsonView::Null => "null".into(),
            JsonView::Text(points) => format!("str:{points:?}"),
            JsonView::Bool(value) => format!("number:{}", u8::from(value)),
            JsonView::Integer(value) => format!("number:{value}"),
            JsonView::Float(value) if value.is_finite() && value.fract() == 0. => {
                format!("number:{value:.0}")
            }
            JsonView::Float(_) => format!("float:{}", value.to_json().unwrap()),
            JsonView::Array(_) | JsonView::Object(_) => {
                return Err(Error::new("TypeError", "unhashable cache argument"))
            }
        });
    }
    Ok(format!("tuple:{}", serde_json::to_string(&keys).unwrap()))
}
