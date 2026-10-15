//! Cargo JSONL test artifact listing for the native ARM validation workflow.
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};

fn truthy(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value.bytes().any(|b| b != b'-' && b != b'0'),
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}

fn parse(line: &str) -> Result<JsonValue> {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for c in line.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            if c == '[' || c == '{' {
                depth += 1;
                if depth > 9997 {
                    return Err("original JSON decoder nesting limit exceeded".into());
                }
            } else if c == ']' || c == '}' {
                depth = depth.saturating_sub(1);
            }
        }
    }
    Ok(JsonValue::parse(line)?)
}

/// Preserve the original eager decoding, ordered selection, and Linux UTF-8
/// surrogateescape output. Invalid later records never publish an earlier path.
pub fn select(input: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(input)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let rows = text
        .split_inclusive('\n')
        .map(parse)
        .collect::<Result<Vec<_>>>()?;
    let mut output = vec![];
    let mut count = 0;
    for row in rows {
        if !row.is_object() {
            return Err("Cargo record does not support object lookup".into());
        }
        if !row
            .get("reason")
            .is_some_and(|v| v.text_eq("compiler-artifact"))
        {
            continue;
        }
        let Some(profile) = row.get("profile") else {
            continue;
        };
        if !profile.is_object() {
            return Err("Cargo profile does not support object lookup".into());
        }
        if !profile.get("test").as_ref().is_some_and(truthy) {
            continue;
        }
        let Some(path) = row.get("executable").filter(truthy) else {
            continue;
        };
        let JsonView::Text(points) = path.view() else {
            return Err("selected Cargo executable is not a string".into());
        };
        if count != 0 {
            output.push(b'\n');
        }
        count += 1;
        for &point in points {
            if (0xdc80..=0xdcff).contains(&point) {
                output.push((point - 0xdc00) as u8);
            } else {
                let c = char::from_u32(point).ok_or("unencodable output surrogate")?;
                output.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            }
        }
    }
    output.push(b'\n');
    Ok(output)
}
