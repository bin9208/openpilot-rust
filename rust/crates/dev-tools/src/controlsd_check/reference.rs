//! Caller-specific adapter; scalar comparison and full Event projection are shared with paramsd.
use super::{event_reference, numeric};
use openpilot_dev_tools::{controlsd_fixture_generate, seal::Result};
use serde_json::{json, Value};

/// Python str.splitlines(keepends=True), including CRLF and Unicode separators.
pub fn strip_original_prints(text: &str) -> String {
    let mut out = String::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if !matches!(
            c,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            continue;
        }
        let mut end = i + c.len_utf8();
        if c == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
            chars.next();
            end += 1;
        }
        let line = &text[start..end];
        if !line.starts_with("DBC: ") && line != "Using Hyundai CAN FD checksum\n" {
            out.push_str(line);
        }
        start = end;
    }
    if !text[start..].starts_with("DBC: ") {
        out.push_str(&text[start..]);
    }
    out
}
pub fn normalize(actual: &mut Value) -> Result<()> {
    for case in actual.as_array_mut().ok_or("trace must be list")? {
        for row in case
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .ok_or("missing rows")?
        {
            let row = row.as_object_mut().ok_or("row must be object")?;
            if row.contains_key("publications") {
                return Err("predecoded native publications forbidden".into());
            }
            let packets = row.remove("packets").ok_or("missing packets")?;
            let mut publications = Vec::new();
            for packet in packets.as_array().ok_or("packets must be list")? {
                let bytes: Vec<u8> = serde_json::from_value(packet.clone())?;
                publications.push(json!({"service":controlsd_fixture_generate::service(&bytes)?,"event":event_reference::event_dict(&bytes)?}));
            }
            row.insert("publications".into(), Value::Array(publications));
        }
    }
    Ok(())
}
/// Compare decoded rows without deleting any other field or changing order.
/// The executable calls normalize first; tests can isolate comparison.
pub fn compare(expected: &Value, actual: Value) -> Result<(Value, Value)> {
    let mut expected = expected.clone();
    for case in expected
        .as_array_mut()
        .ok_or("original trace must be list")?
    {
        let printed = case.get_mut("printed").ok_or("missing original printed")?;
        *printed = json!(strip_original_prints(
            printed.as_str().ok_or("printed must be text")?
        ));
    }
    let mut stats = numeric::Stats::default();
    numeric::compare(&expected, &actual, "controls", &mut stats)?;
    if stats.numeric_comparisons == 0 {
        return Err("original max(errors) undefined: no finite float comparisons".into());
    }
    let cases = actual.as_array().ok_or("native trace must be list")?;
    let frames = cases
        .iter()
        .try_fold(0usize, |count, case| -> Result<usize> {
            Ok(count
                + case
                    .get("rows")
                    .and_then(Value::as_array)
                    .ok_or("missing native rows")?
                    .len())
        })?;
    let result = json!({"pass":true,"cases":cases.len(),"frames":frames,"numeric_comparisons":stats.numeric_comparisons,"max_absolute_error":stats.max_absolute_error});
    Ok((actual, result))
}
/// Inverse of the shared typed Python JSON parser, without leaking private sentinels.
pub fn python_json(value: &Value) -> Result<Vec<u8>> {
    fn write(value: &Value, out: &mut Vec<u8>) -> Result<()> {
        match value {
            Value::String(_) if numeric::number(value).is_some_and(|n| !n.is_finite()) => {
                let n = numeric::number(value).ok_or("typed float missing")?;
                out.extend_from_slice(if n.is_nan() {
                    b"NaN"
                } else if n.is_sign_positive() {
                    b"Infinity"
                } else {
                    b"-Infinity"
                });
            }
            Value::Array(values) => {
                out.push(b'[');
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    write(v, out)?;
                }
                out.push(b']');
            }
            Value::Object(fields) => {
                out.push(b'{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    out.extend(serde_json::to_vec(k)?);
                    out.push(b':');
                    write(v, out)?;
                }
                out.push(b'}');
            }
            _ => out.extend(serde_json::to_vec(value)?),
        }
        Ok(())
    }
    let mut out = Vec::new();
    write(value, &mut out)?;
    out.push(b'\n');
    Ok(out)
}
