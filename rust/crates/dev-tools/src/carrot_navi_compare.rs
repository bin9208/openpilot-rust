//! Port of the Carrot Navi workflow's strict saved-capture comparison.
//! Original traffic controllers and the independent Python oracle remain live.
use crate::{control_tools_policy_check::json, seal::Result};
use openpilot_log_format::{JsonValue, JsonView};
use std::{fs, path::Path};

fn integer(value: &str) -> &str {
    if value == "-0" {
        "0"
    } else {
        value
    }
}

fn int_float(integer_value: &str, float: f64) -> bool {
    float.is_finite()
        && float.fract() == 0.0
        && integer(integer_value)
            == if float == 0.0 {
                "0".into()
            } else {
                format!("{float:.0}")
            }
}

/// Match radarcan_exact.assert_exact(actual, expected), including its directional
/// int/float rules. Expected floats compare binary64 bits, including signed zero;
/// expected integers compare numeric values without rounding the integer.
pub fn assert_exact(actual: &JsonValue, expected: &JsonValue, path: &str) -> Result<()> {
    let mut pending = vec![(actual.clone(), expected.clone(), path.to_owned())];
    while let Some((a, e, path)) = pending.pop() {
        let same = match (a.view(), e.view()) {
            (JsonView::Null, JsonView::Null) => true,
            (JsonView::Bool(a), JsonView::Bool(e)) => a == e,
            (JsonView::Text(a), JsonView::Text(e)) => a == e,
            (JsonView::Integer(a), JsonView::Integer(e)) => integer(a) == integer(e),
            (JsonView::Float(a), JsonView::Integer(e)) => int_float(e, a),
            (JsonView::Float(a), JsonView::Float(e)) => a.to_bits() == e.to_bits(),
            (JsonView::Integer(a), JsonView::Float(e)) => {
                // CPython struct.pack('<d', huge_int) raises on overflow rather
                // than allowing a conversion to Infinity to pass.
                let a = integer(a).parse::<f64>()?;
                a.is_finite() && a.to_bits() == e.to_bits()
            }
            (JsonView::Array(a), JsonView::Array(e)) if a.len() == e.len() => {
                for (i, (a, e)) in a.into_iter().zip(e).enumerate().rev() {
                    pending.push((a, e, format!("{path}/{i}")));
                }
                true
            }
            (JsonView::Object(a), JsonView::Object(e)) if a.len() == e.len() => {
                for (key, e) in e.into_iter().rev() {
                    let (_, a) = a
                        .iter()
                        .find(|(k, _)| *k == key)
                        .ok_or_else(|| format!("{path}: object keys differ"))?;
                    let key = JsonValue::codepoints(key.to_vec())
                        .ok_or("invalid key")?
                        .to_json()?;
                    pending.push((a.clone(), e, format!("{path}/{key}")));
                }
                true
            }
            _ => false,
        };
        if !same {
            return Err(format!(
                "{path}: strict comparison differs: actual={} expected={}",
                a.to_json()?,
                e.to_json()?
            )
            .into());
        }
    }
    Ok(())
}

fn key_is(key: &[u32], name: &str) -> bool {
    key.iter().copied().eq(name.chars().map(u32::from))
}

fn truthy(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => integer(v) != "0",
        JsonView::Float(v) => v != 0.0,
        JsonView::Text(v) => !v.is_empty(),
        JsonView::Array(v) => !v.is_empty(),
        JsonView::Object(v) => !v.is_empty(),
    }
}

fn replace(text: &[u32], token: &[u32], replacement: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    if token.is_empty() {
        out.extend_from_slice(replacement);
        for &point in text {
            out.push(point);
            out.extend_from_slice(replacement);
        }
        return out;
    }
    let mut i = 0;
    while i < text.len() {
        if text[i..].starts_with(token) {
            out.extend_from_slice(replacement);
            i += token.len();
        } else {
            out.push(text[i]);
            i += 1;
        }
    }
    out
}

#[derive(Default)]
struct Normalizer {
    // Python dict insertion order determines replacement order. Tokens are
    // registered only on encounter, not pre-scanned or replaced in object keys.
    sessions: Vec<(Vec<u32>, Vec<u32>)>,
}

impl Normalizer {
    fn walk(&mut self, value: &JsonValue, key: &[u32]) -> Result<String> {
        match value.view() {
            JsonView::Object(fields) => {
                let mut out = Vec::new();
                for (key, value) in fields {
                    if !key_is(key, "cereal_publish_count") {
                        let encoded_key = JsonValue::codepoints(key.to_vec())
                            .ok_or("invalid key")?
                            .to_json()?;
                        out.push(format!("{encoded_key}: {}", self.walk(&value, key)?));
                    }
                }
                return Ok(format!("{{{}}}", out.join(", ")));
            }
            JsonView::Array(values) => {
                let values = values
                    .iter()
                    .map(|v| self.walk(v, &[]))
                    .collect::<Result<Vec<_>>>()?;
                return Ok(format!("[{}]", values.join(", ")));
            }
            _ => {}
        }
        let mut value = value.clone();
        if let JsonView::Text(text) = value.view() {
            if key_is(key, "session_id") && !self.sessions.iter().any(|(token, _)| token == text) {
                self.sessions.push((
                    text.to_vec(),
                    format!("<session{}>", self.sessions.len())
                        .chars()
                        .map(u32::from)
                        .collect(),
                ));
            }
            let mut text = text.to_vec();
            for (token, replacement) in &self.sessions {
                text = replace(&text, token, replacement);
            }
            value = JsonValue::codepoints(text).ok_or("invalid normalized text")?;
        }
        if key_is(key, "port") {
            let fixed = match value.view() {
                JsonView::Integer(v) => v == "7714",
                JsonView::Float(v) => v == 7714.0,
                _ => false,
            };
            if !fixed {
                return Ok(JsonValue::text("<bound-port>").to_json()?);
            }
        }
        if [
            "last_received_at_ms",
            "received_at_ms",
            "last_cereal_publish_mono_ns",
        ]
        .iter()
        .any(|name| key_is(key, name))
            && truthy(&value)
        {
            // isinstance(value, int) in the original also admits True. Zero,
            // False, null and empty strings remain unprojected and compare as-is.
            let positive = match value.view() {
                JsonView::Integer(v) => !v.starts_with('-') && v != "0",
                JsonView::Bool(v) => v,
                _ => false,
            };
            if !positive {
                return Err("nonzero Carrot Navi clock must be a positive Python integer".into());
            }
            return Ok(JsonValue::text("<positive-clock>").to_json()?);
        }
        Ok(value.to_json()?)
    }
}

/// Apply exactly the inherited workflow projections to one complete row array.
pub fn normalize(rows: &JsonValue) -> Result<JsonValue> {
    Ok(JsonValue::parse(&Normalizer::default().walk(rows, &[])?)?)
}

fn read(root: &Path, relative: &str) -> Result<JsonValue> {
    // Path.read_text() followed by json.loads uses decoded UTF-8 text, not
    // json.loads(bytes) encoding autodetection. Keep that distinction here.
    json::parse_text(&fs::read_to_string(root.join(relative))?)
}

fn length(value: &JsonValue) -> Result<usize> {
    match value.view() {
        JsonView::Array(v) => Ok(v.len()),
        JsonView::Object(v) => Ok(v.len()),
        JsonView::Text(v) => Ok(v.len()),
        _ => Err("comparison value has no Python length".into()),
    }
}

fn wires(value: &JsonValue) -> Result<JsonValue> {
    let JsonView::Array(rows) = value.view() else {
        return Err("discovery must be an array".into());
    };
    let rows = rows
        .iter()
        .map(|r| -> Result<String> {
            let value = r.get("wire_hex").ok_or("discovery wire_hex absent")?;
            Ok(value.to_json()?)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(JsonValue::parse(&format!("[{}]", rows.join(", ")))?)
}

/// Compare current saved independent captures. Writes the inherited normalized
/// rows and success receipt only after the corresponding original assertions.
pub fn compare_root(root: &Path) -> Result<()> {
    assert_exact(
        &read(root, "policy-native.json")?,
        &read(root, "policy-source.json")?,
        "policy",
    )?;
    for (lane, count) in [("socket", 56), ("lifecycle", 21)] {
        let source = normalize(&read(root, &format!("{lane}-source/rows.json"))?)?;
        let native = normalize(&read(root, &format!("{lane}-native/rows.json"))?)?;
        if length(&source)? != count || length(&native)? != count {
            return Err(format!("{lane}: expected {count} observations").into());
        }
        assert_exact(&native, &source, lane)?;
        assert_exact(
            &read(root, &format!("{lane}-native/result.json"))?,
            &read(root, &format!("{lane}-source/result.json"))?,
            &format!("{lane}/result"),
        )?;
        for (kind, rows) in [("source", source), ("native", native)] {
            fs::write(
                root.join(format!("{lane}-{kind}-normalized.json")),
                json::pretty(&rows, 0)?,
            )?;
        }
    }
    for lane in ["startup", "exhaust"] {
        assert_exact(
            &read(root, &format!("{lane}-native/result.json"))?,
            &read(root, &format!("{lane}-source/result.json"))?,
            lane,
        )?;
    }
    assert_exact(
        &wires(&read(root, "startup-native/discovery.json")?)?,
        &wires(&read(root, "startup-source/discovery.json")?)?,
        "discovery/wire",
    )?;
    let receipt = JsonValue::parse(&format!("{{\"status\":\"pass\",\"policy_cases\":{},\"socket_observations\":56,\"lifecycle_observations\":21,\"discovery_byte_equal\":true}}", length(&read(root, "policy-source.json")?)?))?;
    fs::write(root.join("comparison.json"), json::pretty(&receipt, 0)?)?;
    Ok(())
}

/// Native saved-capture CLI: check-carrot-navi-comparison CAPTURE_ROOT.
pub fn cli() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let root = args.next().ok_or("expected capture root")?;
    if args.next().is_some() {
        return Err("expected exactly one capture root".into());
    }
    compare_root(Path::new(&root))
}
