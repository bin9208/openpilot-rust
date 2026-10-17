//! Python JSON byte decoding and equality used by the original comparator.
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};

/// Parse already-decoded text without allowing byte-encoding autodetection.
pub(crate) fn parse_text(text: &str) -> Result<JsonValue> {
    if text.starts_with('\u{feff}') || text.contains('\0') {
        return Err("unexpected BOM or raw NUL in JSON text".into());
    }
    parse(text.as_bytes())
}

pub(super) fn parse(bytes: &[u8]) -> Result<JsonValue> {
    let (width, little, skip) = if bytes.starts_with(b"\xff\xfe\0\0") {
        (4, true, 4)
    } else if bytes.starts_with(b"\0\0\xfe\xff") {
        (4, false, 4)
    } else if bytes.starts_with(b"\xff\xfe") {
        (2, true, 2)
    } else if bytes.starts_with(b"\xfe\xff") {
        (2, false, 2)
    } else if bytes.starts_with(b"\xef\xbb\xbf") {
        (1, false, 3)
    } else if bytes.len() >= 4 && bytes[0] == 0 {
        (if bytes[1] == 0 { 4 } else { 2 }, false, 0)
    } else if bytes.len() >= 4 && bytes[1] == 0 {
        (if bytes[2] == 0 && bytes[3] == 0 { 4 } else { 2 }, true, 0)
    } else if bytes.len() == 2 && bytes[0] == 0 {
        (2, false, 0)
    } else if bytes.len() == 2 && bytes[1] == 0 {
        (2, true, 0)
    } else {
        (1, false, 0)
    };
    let bytes = &bytes[skip..];
    let mut points = Vec::new();
    if width == 1 {
        let mut i = 0;
        while i < bytes.len() {
            let first = bytes[i];
            let (n, mut point, min) = match first {
                0..=127 => (1, u32::from(first), 0),
                0xc2..=0xdf => (2, u32::from(first & 31), 0x80),
                0xe0..=0xef => (3, u32::from(first & 15), 0x800),
                0xf0..=0xf4 => (4, u32::from(first & 7), 0x10000),
                _ => return Err("invalid UTF-8 JSON".into()),
            };
            if i + n > bytes.len() {
                return Err("truncated UTF-8 JSON".into());
            }
            for &byte in &bytes[i + 1..i + n] {
                if byte & 0xc0 != 0x80 {
                    return Err("invalid UTF-8 continuation".into());
                }
                point = (point << 6) | u32::from(byte & 63);
            }
            if point < min || point > 0x10ffff {
                return Err("invalid UTF-8 codepoint".into());
            }
            points.push(point);
            i += n;
        }
    } else {
        if !bytes.len().is_multiple_of(width) {
            return Err("truncated Unicode JSON".into());
        }
        let mut units = Vec::new();
        for chunk in bytes.chunks_exact(width) {
            let point = if width == 2 {
                let v = [chunk[0], chunk[1]];
                u32::from(if little {
                    u16::from_le_bytes(v)
                } else {
                    u16::from_be_bytes(v)
                })
            } else {
                let v = [chunk[0], chunk[1], chunk[2], chunk[3]];
                if little {
                    u32::from_le_bytes(v)
                } else {
                    u32::from_be_bytes(v)
                }
            };
            if point > 0x10ffff {
                return Err("invalid Unicode codepoint".into());
            }
            units.push(point);
        }
        let mut i = 0;
        while i < units.len() {
            let point = units[i];
            if width == 2
                && (0xd800..=0xdbff).contains(&point)
                && units
                    .get(i + 1)
                    .is_some_and(|v| (0xdc00..=0xdfff).contains(v))
            {
                points.push(0x10000 + ((point - 0xd800) << 10) + (units[i + 1] - 0xdc00));
                i += 2;
            } else {
                points.push(point);
                i += 1;
            }
        }
    }
    // Match the pinned Python 3.12 command's C decoder admission separately
    // from its much shallower indent=2 receipt serializer. Quotes/escapes do
    // not contribute to structural depth. The parser still validates syntax.
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for &point in &points {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && point == 92 {
            escaped = true;
            continue;
        }
        if point == 34 {
            quoted = !quoted;
            continue;
        }
        if !quoted {
            if point == 91 || point == 123 {
                depth += 1;
                if depth > 9997 {
                    return Err("Python JSON nesting limit exceeded".into());
                }
            } else if point == 93 || point == 125 {
                depth = depth.saturating_sub(1);
            }
        }
    }
    Ok(JsonValue::parse_codepoints(&points)?)
}

fn normalized_integer(value: &str) -> &str {
    if value == "-0" {
        "0"
    } else {
        value
    }
}
fn int_float(integer: &str, float: f64) -> bool {
    float.is_finite()
        && float.fract() == 0.0
        && normalized_integer(integer)
            == if float == 0.0 {
                "0".into()
            } else {
                format!("{float:.0}")
            }
}
pub(crate) fn equal(a: &JsonValue, b: &JsonValue) -> bool {
    // json.loads shares its NaN constant between documents. Container equality
    // uses identity before numeric equality; top-level NaN steps still differ.
    let mut pending = vec![(a.clone(), b.clone(), false)];
    while let Some((a, b, nested)) = pending.pop() {
        let same = match (a.view(), b.view()) {
            (JsonView::Null, JsonView::Null) => true,
            (JsonView::Bool(a), JsonView::Bool(b)) => a == b,
            (JsonView::Integer(a), JsonView::Integer(b)) => {
                normalized_integer(a) == normalized_integer(b)
            }
            (JsonView::Float(a), JsonView::Float(b)) => {
                a == b || (nested && a.is_nan() && b.is_nan())
            }
            (JsonView::Integer(a), JsonView::Float(b))
            | (JsonView::Float(b), JsonView::Integer(a)) => int_float(a, b),
            (JsonView::Bool(a), JsonView::Integer(b))
            | (JsonView::Integer(b), JsonView::Bool(a)) => {
                normalized_integer(b) == if a { "1" } else { "0" }
            }
            (JsonView::Bool(a), JsonView::Float(b)) | (JsonView::Float(b), JsonView::Bool(a)) => {
                f64::from(u8::from(a)) == b
            }
            (JsonView::Text(a), JsonView::Text(b)) => a == b,
            (JsonView::Array(a), JsonView::Array(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                pending.extend(a.into_iter().zip(b).map(|(a, b)| (a, b, true)));
                true
            }
            (JsonView::Object(a), JsonView::Object(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                for (key, value) in a {
                    let Some((_, other)) = b.iter().find(|(other, _)| *other == key) else {
                        return false;
                    };
                    pending.push((value, other.clone(), true));
                }
                true
            }
            _ => false,
        };
        if !same {
            return false;
        }
    }
    true
}
pub(crate) fn sequence(value: &JsonValue) -> Result<Vec<JsonValue>> {
    match value.view() {
        JsonView::Array(values) => Ok(values),
        JsonView::Object(values) => values
            .into_iter()
            .map(|(key, _)| {
                JsonValue::codepoints(key.to_vec()).ok_or_else(|| "invalid object key".into())
            })
            .collect(),
        JsonView::Text(value) => value
            .iter()
            .map(|&v| JsonValue::codepoints(vec![v]).ok_or_else(|| "invalid text".into()))
            .collect(),
        _ => Err("JSON value is not iterable and has no length".into()),
    }
}
pub(crate) fn pretty(value: &JsonValue, initial_depth: usize) -> Result<String> {
    // Original control CLI's captured serializer context; other fixed callers
    // provide their independently measured limit through the shared writer.
    pretty_with_limit(value, initial_depth, 995)
}
pub(crate) fn pretty_with_limit(
    value: &JsonValue,
    initial_depth: usize,
    container_limit: usize,
) -> Result<String> {
    enum Part {
        Node(JsonValue, usize),
        Text(String),
    }
    let mut pending = vec![Part::Node(value.clone(), initial_depth)];
    let mut out = String::new();
    while let Some(part) = pending.pop() {
        let (value, depth) = match part {
            Part::Text(text) => {
                out.push_str(&text);
                continue;
            }
            Part::Node(v, d) => (v, d),
        };
        if depth > container_limit {
            return Err("Python indented JSON nesting limit exceeded".into());
        }
        let (open, close, entries) = match value.view() {
            JsonView::Array(v) => (
                "[",
                "]",
                v.into_iter().map(|v| (None, v)).collect::<Vec<_>>(),
            ),
            JsonView::Object(v) => (
                "{",
                "}",
                v.into_iter().map(|(k, v)| (Some(k.to_vec()), v)).collect(),
            ),
            _ => {
                out.push_str(&value.to_json()?);
                continue;
            }
        };
        if depth == container_limit {
            return Err("Python indented JSON container nesting limit exceeded".into());
        }
        out.push_str(open);
        if entries.is_empty() {
            out.push_str(close);
            continue;
        }
        pending.push(Part::Text(format!("\n{}{close}", "  ".repeat(depth))));
        for (i, (key, v)) in entries.into_iter().enumerate().rev() {
            pending.push(Part::Node(v, depth + 1));
            let mut prefix = format!(
                "{}\n{}",
                if i == 0 { "" } else { "," },
                "  ".repeat(depth + 1)
            );
            if let Some(key) = key {
                prefix.push_str(&JsonValue::codepoints(key).ok_or("invalid key")?.to_json()?);
                prefix.push_str(": ");
            }
            pending.push(Part::Text(prefix));
        }
    }
    Ok(out)
}
