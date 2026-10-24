//! check_paramsd.compare, including Python JSON's nonfinite tokens and the
//! asymmetric numpy.isclose(expected, actual, equal_nan=True) operation order.
use openpilot_dev_tools::seal::Result;
use serde_json::{json, Value};
const NAN: &str = "__paramsd_estimator_nan_f64__";
const POS: &str = "__paramsd_estimator_posinf_f64__";
const NEG: &str = "__paramsd_estimator_neginf_f64__";

pub fn float(value: f64) -> Value {
    if value.is_nan() {
        json!(NAN)
    } else if value == f64::INFINITY {
        json!(POS)
    } else if value == f64::NEG_INFINITY {
        json!(NEG)
    } else {
        json!(value)
    }
}
pub fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(v) => v.as_f64(),
        Value::Bool(v) => Some(f64::from(u8::from(*v))),
        Value::String(v) if v == NAN => Some(f64::NAN),
        Value::String(v) if v == POS => Some(f64::INFINITY),
        Value::String(v) if v == NEG => Some(f64::NEG_INFINITY),
        _ => None,
    }
}
fn reserved(text: &str) -> bool {
    matches!(text, NAN | POS | NEG)
}

/// Parse only the three nonstandard float literals accepted by Python json.
/// Quoted values/keys are decoded first so even escaped private sentinels are
/// rejected; a source string can never masquerade as a float token.
pub fn parse_python(raw: &[u8]) -> Result<Value> {
    let mut translated = Vec::with_capacity(raw.len());
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'"' {
            let start = index;
            index += 1;
            let mut escaped = false;
            let mut closed = false;
            while index < raw.len() {
                let byte = raw[index];
                index += 1;
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err("unterminated Python JSON string".into());
            }
            let decoded: String = serde_json::from_slice(&raw[start..index])?;
            if reserved(&decoded) {
                return Err("reserved estimator float sentinel in source JSON".into());
            }
            translated.extend_from_slice(&raw[start..index]);
            continue;
        }
        let token = [
            (b"-Infinity".as_slice(), NEG),
            (b"Infinity".as_slice(), POS),
            (b"NaN".as_slice(), NAN),
        ]
        .into_iter()
        .find(|(token, _)| raw[index..].starts_with(token));
        if let Some((token, sentinel)) = token {
            let before = index == 0
                || raw[index - 1].is_ascii_whitespace()
                || b"[:,".contains(&raw[index - 1]);
            let end = index + token.len();
            let after =
                end == raw.len() || raw[end].is_ascii_whitespace() || b",]}".contains(&raw[end]);
            if !before || !after {
                return Err("invalid Python JSON float token boundary".into());
            }
            translated.extend_from_slice(&serde_json::to_vec(sentinel)?);
            index = end;
        } else {
            translated.push(raw[index]);
            index += 1;
        }
    }
    Ok(serde_json::from_slice(&translated)?)
}

pub fn is_close(expected: f64, actual: f64, tolerance: f64) -> bool {
    // np.isclose: finite-reference relative term uses ACTUAL, not expected.
    // Keep separate subtraction/multiplication/addition; no fused multiply-add.
    ((expected - actual).abs() <= tolerance + tolerance * actual.abs() && actual.is_finite())
        || expected == actual
        || (expected.is_nan() && actual.is_nan())
}
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Stats {
    pub numeric_comparisons: u64,
    pub max_absolute_error: f64,
}
fn integer(value: &Value) -> Option<i128> {
    match value {
        Value::Bool(v) => Some(i128::from(u8::from(*v))),
        Value::Number(v) => v
            .as_i64()
            .map(i128::from)
            .or_else(|| v.as_u64().map(i128::from)),
        _ => None,
    }
}
fn scalar_equal(expected: &Value, actual: &Value) -> bool {
    if let Some(left) = integer(expected) {
        if let Some(right) = integer(actual) {
            return left == right;
        }
        if let Some(right) = number(actual) {
            // Python int == float compares exact mathematical integers, not a
            // rounded int->f64 conversion (important beyond 2**53).
            return right.is_finite() && right.fract() == 0. && right as i128 == left;
        }
        return false;
    }
    expected == actual
}

pub fn compare(expected: &Value, actual: &Value, path: &str, stats: &mut Stats) -> Result<()> {
    match expected {
        Value::Object(fields) => {
            let found = actual
                .as_object()
                .ok_or_else(|| format!("{path}: expected object"))?;
            if fields.len() != found.len() || fields.keys().any(|key| !found.contains_key(key)) {
                return Err(format!("{path}: object key set differs").into());
            }
            for (key, left) in fields {
                compare(left, &found[key], &format!("{path}.{key}"), stats)?;
            }
        }
        Value::Array(items) => {
            let found = actual
                .as_array()
                .ok_or_else(|| format!("{path}: expected list"))?;
            if items.len() != found.len() {
                return Err(format!("{path}: list length differs").into());
            }
            for (index, (left, right)) in items.iter().zip(found).enumerate() {
                compare(left, right, &format!("{path}[{index}]"), stats)?;
            }
        }
        Value::Number(v) if v.is_f64() => compare_float(
            number(expected).ok_or("expected float")?,
            actual,
            path,
            stats,
        )?,
        Value::String(v) if reserved(v) => compare_float(
            number(expected).ok_or("expected float token")?,
            actual,
            path,
            stats,
        )?,
        _ => {
            if !scalar_equal(expected, actual) {
                return Err(format!("{path}: scalar differs: {expected} != {actual}").into());
            }
        }
    }
    Ok(())
}
fn compare_float(expected: f64, actual: &Value, path: &str, stats: &mut Stats) -> Result<()> {
    let actual = number(actual).ok_or_else(|| format!("{path}: expected numeric value"))?;
    let tolerance = if path.contains(".event.") && !path.contains(".debugFilterState.") {
        2e-6
    } else {
        1e-9
    };
    if !is_close(expected, actual, tolerance) {
        return Err(format!("{path}: {expected:?} != {actual:?}").into());
    }
    if expected.is_finite() && actual.is_finite() {
        stats.numeric_comparisons += 1;
        stats.max_absolute_error = stats.max_absolute_error.max((expected - actual).abs());
    }
    Ok(())
}
