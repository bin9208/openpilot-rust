//! JSON-domain adaptation of CPython Lib/pprint.py's default width80/indent1,
//! compact=False/sort_dicts=True behavior. Original author: Fred L. Drake, Jr.
//! PSF license/notice: ../../changed_files_check/LICENSE-CPYTHON. No Python execution.
//! JSON cannot contain cycles, arbitrary objects, tuples or nonstring dict keys.
use crate::developer_ssh::Result;
use openpilot_log_format::{JsonValue, JsonView};

fn len(text: &str) -> usize {
    text.chars().count()
}
fn scalar_repr(value: &JsonValue) -> Result<String> {
    // Reuse the repository's Python scalar repr (including quotes, Unicode,
    // signed zero and nonfinite floats). A one-element list requests repr, not str.
    let wrapped = JsonValue::parse(&format!("[{}]", value.to_json()?))?;
    let points = openpilot_runtime_version::python_str(&wrapped)?;
    points[1..points.len() - 1]
        .iter()
        .map(|&p| char::from_u32(p).ok_or_else(|| "unencodable repr".into()))
        .collect()
}
fn string_repr(points: &[u32]) -> Result<String> {
    scalar_repr(&JsonValue::codepoints(points.to_vec()).ok_or("invalid Unicode point")?)
}
fn compact(value: &JsonValue) -> Result<String> {
    match value.view() {
        JsonView::Array(values) => Ok(format!(
            "[{}]",
            values
                .iter()
                .map(compact)
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        JsonView::Object(mut entries) => {
            entries.sort_by_key(|(key, _)| *key);
            Ok(format!(
                "{{{}}}",
                entries
                    .iter()
                    .map(|(key, value)| Ok(format!("{}: {}", string_repr(key)?, compact(value)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
        _ => scalar_repr(value),
    }
}
fn whitespace(point: u32) -> bool {
    char::from_u32(point).is_some_and(char::is_whitespace) || (0x1c..=0x1f).contains(&point)
}
fn lines(points: &[u32]) -> Vec<&[u32]> {
    let (mut start, mut index) = (0, 0);
    let mut lines = vec![];
    while index < points.len() {
        if matches!(points[index], 10..=13 | 0x1c..=0x1e | 0x85 | 0x2028 | 0x2029) {
            let crlf = points[index] == 13 && points.get(index + 1) == Some(&10);
            index += if crlf { 2 } else { 1 };
            lines.push(&points[start..index]);
            start = index;
        } else {
            index += 1;
        }
    }
    if start < points.len() {
        lines.push(&points[start..]);
    }
    lines
}
fn words(points: &[u32]) -> Vec<&[u32]> {
    // re.findall(r'\S*\s*', line), excluding its final empty match.
    let mut index = 0;
    let mut parts = vec![];
    while index < points.len() {
        let start = index;
        while index < points.len() && !whitespace(points[index]) {
            index += 1;
        }
        while index < points.len() && whitespace(points[index]) {
            index += 1;
        }
        parts.push(&points[start..index]);
    }
    parts
}
fn format_string(points: &[u32], indent: usize, allowance: usize, level: usize) -> Result<String> {
    let top = level == 0;
    let indent = indent + usize::from(top);
    let allowance = allowance + usize::from(top);
    let max_width = 80_i64 - indent as i64;
    let lines = lines(points);
    let mut chunks = vec![];
    let mut last_repr = string_repr(points)?;
    for (i, line) in lines.iter().enumerate() {
        last_repr = string_repr(line)?;
        let last_line = i + 1 == lines.len();
        let width = max_width - if last_line { allowance as i64 } else { 0 };
        if len(&last_repr) as i64 <= width {
            chunks.push(last_repr.clone());
        } else {
            let parts = words(line);
            let mut current = vec![];
            for (j, part) in parts.iter().enumerate() {
                let mut candidate = current.clone();
                candidate.extend_from_slice(part);
                let width = max_width
                    - if last_line && j + 1 == parts.len() {
                        allowance as i64
                    } else {
                        0
                    };
                if len(&string_repr(&candidate)?) as i64 > width {
                    if !current.is_empty() {
                        chunks.push(string_repr(&current)?);
                    }
                    current = part.to_vec();
                } else {
                    current = candidate;
                }
            }
            if !current.is_empty() {
                chunks.push(string_repr(&current)?);
            }
        }
    }
    if chunks.len() <= 1 {
        return Ok(last_repr);
    }
    let body = chunks.join(&format!("\n{}", " ".repeat(indent)));
    Ok(if top { format!("({body})") } else { body })
}
fn format(value: &JsonValue, indent: usize, allowance: usize, level: usize) -> Result<String> {
    let repr = compact(value)?;
    if len(&repr) as i64 <= 80_i64 - indent as i64 - allowance as i64 {
        return Ok(repr);
    }
    match value.view() {
        JsonView::Object(mut entries) => {
            entries.sort_by_key(|(key, _)| *key);
            let mut result = String::from("{");
            for (i, (key, value)) in entries.iter().enumerate() {
                if i > 0 {
                    result.push_str(&format!(",\n{}", " ".repeat(indent + 1)));
                }
                let key = string_repr(key)?;
                result.push_str(&key);
                result.push_str(": ");
                result.push_str(&format(
                    value,
                    indent + 1 + len(&key) + 2,
                    if i + 1 == entries.len() {
                        allowance + 1
                    } else {
                        1
                    },
                    level + 1,
                )?);
            }
            result.push('}');
            Ok(result)
        }
        JsonView::Array(values) => {
            let mut result = String::from("[");
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    result.push_str(&format!(",\n{}", " ".repeat(indent + 1)));
                }
                result.push_str(&format(
                    value,
                    indent + 1,
                    if i + 1 == values.len() {
                        allowance + 1
                    } else {
                        1
                    },
                    level + 1,
                )?);
            }
            result.push(']');
            Ok(result)
        }
        JsonView::Text(points) => format_string(points, indent, allowance, level),
        _ => Ok(repr),
    }
}
pub(super) fn pformat(value: &JsonValue) -> Result<String> {
    format(value, 0, 0, 0)
}
