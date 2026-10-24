//! Numeric reports retain original bytes except independently recomputed source
//! pose floats under the immutable preimplementation pose budget. Every other
//! scalar, key order, whitespace, punctuation and error selector remains exact.
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::ops::Range;

type Result<T> = std::result::Result<T, String>;
#[derive(Default, Debug)]
pub struct Summary {
    pub source_float_differences: usize,
    pub derived_diagnostic_differences: usize,
}
#[derive(Clone, Copy)]
enum Permit {
    Exact,
    PoseFloat,
    DerivedDiagnostic,
}
#[derive(Default)]
struct WalkState {
    leaves: Vec<Leaf>,
    summary: Summary,
}
struct Leaf {
    want: JsonValue,
    got: JsonValue,
    permit: Permit,
    path: String,
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("missing report {key}"))
}
fn array(value: &JsonValue) -> Result<Vec<JsonValue>> {
    if let JsonView::Array(rows) = value.view() {
        Ok(rows)
    } else {
        Err("expected report array".into())
    }
}
fn float(value: &JsonValue) -> Result<f64> {
    if let JsonView::Float(value) = value.view() {
        Ok(value)
    } else {
        Err("source floating classification changed".into())
    }
}
fn budget() -> Result<(f64, f64)> {
    let raw = include_bytes!("../../../lagd/tests/tolerances.json");
    if format!("{:x}", Sha256::digest(raw))
        != "5f8d76478bf89581b2c7e25e7e122cd61fb3ddcb3cf63f59ece3b2c9c22e766b"
    {
        return Err("original tolerance bytes changed".into());
    }
    let value = JsonValue::parse(std::str::from_utf8(raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let value = field(&value, "pose_and_smoothing")?;
    Ok((
        float(&field(&value, "absolute")?)?,
        float(&field(&value, "relative")?)?,
    ))
}
fn close(want: f64, got: f64, budget: (f64, f64)) -> bool {
    want.is_finite()
        && got.is_finite()
        && (want - got).abs()
            <= budget
                .0
                .max(budget.1 * want.abs())
                .max(budget.1 * got.abs())
}
/// Primitive tokens include dictionary keys. Punctuation/spacing remain between
/// tokens, so substituting permitted scalar tokens cannot hide formatting drift.
fn tokens(text: &str) -> Result<Vec<Range<usize>>> {
    let bytes = text.as_bytes();
    let mut output = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() || b"{}[]():,".contains(&bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        if matches!(bytes[i], b'\'' | b'"') {
            let quote = bytes[i];
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == quote {
                    i += 1;
                    closed = true;
                    break;
                } else {
                    i += 1;
                }
            }
            if !closed {
                return Err("unterminated report string token".into());
            }
        } else {
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && !b"{}[]():,".contains(&bytes[i])
            {
                i += 1;
            }
        }
        output.push(start..i);
    }
    Ok(output)
}
fn substitute(want: &str, got: &str, leaves: &[Leaf]) -> Result<String> {
    let want_tokens = tokens(want)?;
    let got_tokens = tokens(got)?;
    if want_tokens.len() != leaves.len() || got_tokens.len() != leaves.len() {
        return Err("report scalar cardinality changed".into());
    }
    let mut output = String::new();
    let mut end = 0;
    for ((want_range, got_range), leaf) in want_tokens.iter().zip(&got_tokens).zip(leaves) {
        output.push_str(&got[end..got_range.start]);
        let a = &want[want_range.clone()];
        let b = &got[got_range.clone()];
        match leaf.permit {
            Permit::Exact => {
                if a != b {
                    return Err(format!("{}: nonpose report token changed", leaf.path));
                }
                output.push_str(b);
            }
            Permit::PoseFloat => {
                // Even diagnostics must spell the exact value held in their
                // corresponding report source field, not an unrelated number.
                if a != leaf.want.to_json().map_err(|e| e.to_string())?
                    || b != leaf.got.to_json().map_err(|e| e.to_string())?
                {
                    return Err(format!(
                        "{}: pose number token does not match source value",
                        leaf.path
                    ));
                }
                output.push_str(a);
            }
            Permit::DerivedDiagnostic => output.push_str(a),
        }
        end = got_range.end;
    }
    output.push_str(&got[end..]);
    if output != want {
        return Err("report punctuation, whitespace, or ordering changed".into());
    }
    Ok(output)
}
fn arguments(text: &str) -> Result<Vec<Range<usize>>> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
        return Err("failure tuple format changed".into());
    }
    let mut ranges = Vec::new();
    let mut depth = 0usize;
    let mut quote = None;
    let mut i = 1;
    let mut start = 1;
    while i + 1 < bytes.len() {
        let byte = bytes[i];
        if let Some(q) = quote {
            if byte == b'\\' {
                i += 2;
                continue;
            }
            if byte == q {
                quote = None;
            }
        } else if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else if b"([{ ".contains(&byte) && byte != b' ' {
            depth += 1;
        } else if b")] }".contains(&byte) && byte != b' ' {
            depth = depth.checked_sub(1).ok_or("invalid failure nesting")?;
        } else if byte == b',' && depth == 0 {
            ranges.push(start..i);
            start = i + 1;
        }
        i += 1;
    }
    if depth != 0 || quote.is_some() {
        return Err("unbalanced failure tuple".into());
    }
    ranges.push(start..bytes.len() - 1);
    Ok(ranges)
}
fn diagnostic(
    want: &str,
    got: &str,
    original: &JsonValue,
    actual: &JsonValue,
    budget: (f64, f64),
) -> Result<()> {
    let a = arguments(want)?;
    let b = arguments(got)?;
    if a.len() != 3 || b.len() != 3 {
        return Err("failure tuple shape changed".into());
    }
    if want[a[0].clone()] != got[b[0].clone()] || want[a[2].clone()] != got[b[2].clone()] {
        return Err("failure selector or native value changed".into());
    }
    let original_rows = array(&field(original, "records")?)?;
    let actual_rows = array(&field(actual, "records")?)?;
    for (index, row) in original_rows.iter().enumerate() {
        let name = field(row, "case")?.to_utf8().ok_or("case is not text")?;
        if want[a[0].clone()].trim() != format!("'{name}'") {
            continue;
        }
        if field(&field(row, "input")?, "kind")?.to_utf8().as_deref() != Some("pose") {
            return Err("nonpose failure changed".into());
        }
        let mut state = WalkState::default();
        walk(
            &field(row, "source")?,
            &field(
                actual_rows.get(index).ok_or("missing diagnostic record")?,
                "source",
            )?,
            "",
            true,
            budget,
            None,
            &mut state,
        )?;
        substitute(&want[a[1].clone()], &got[b[1].clone()], &state.leaves)?;
        let normalized = format!(
            "{}{}{}",
            &got[..b[1].start],
            &want[a[1].clone()],
            &got[b[1].end..]
        );
        if normalized != want {
            return Err("failure diagnostic punctuation changed".into());
        }
        return Ok(());
    }
    Err("changed failure is not a whole-pose source diagnostic".into())
}
fn walk(
    want: &JsonValue,
    got: &JsonValue,
    path: &str,
    pose: bool,
    budget: (f64, f64),
    reports: Option<(&JsonValue, &JsonValue)>,
    state: &mut WalkState,
) -> Result<()> {
    match (want.view(), got.view()) {
        (JsonView::Object(a), JsonView::Object(b)) => {
            if a.len() != b.len() || a.iter().zip(&b).any(|((x, _), (y, _))| x != y) {
                return Err(format!("{path}: report keys/order changed"));
            }
            for ((key, a), (_, b)) in a.iter().zip(&b) {
                let key_value = JsonValue::codepoints(key.to_vec()).ok_or("invalid report key")?;
                let key_text = key_value.to_utf8().ok_or("surrogate report key")?;
                state.leaves.push(Leaf {
                    want: key_value.clone(),
                    got: key_value,
                    permit: Permit::Exact,
                    path: path.to_owned(),
                });
                let key = key_text;
                let is_pose_source = if !pose && key == "source" && path.starts_with(".records[") {
                    field(&field(want, "input")?, "kind")?.to_utf8().as_deref() == Some("pose")
                } else {
                    false
                };
                walk(
                    a,
                    b,
                    &format!("{path}.{key}"),
                    pose || is_pose_source,
                    budget,
                    reports,
                    state,
                )?;
            }
        }
        (JsonView::Array(a), JsonView::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("{path}: report array cardinality changed"));
            }
            for (index, (a, b)) in a.iter().zip(&b).enumerate() {
                walk(
                    a,
                    b,
                    &format!("{path}[{index}]"),
                    pose,
                    budget,
                    reports,
                    state,
                )?;
            }
        }
        (JsonView::Float(a), JsonView::Float(b)) if pose => {
            if !close(a, b, budget) {
                return Err(format!(
                    "{path}: pose reference difference exceeds original budget"
                ));
            }
            if want.to_json().map_err(|e| e.to_string())?
                != got.to_json().map_err(|e| e.to_string())?
            {
                state.summary.source_float_differences += 1;
            }
            state.leaves.push(Leaf {
                want: want.clone(),
                got: got.clone(),
                permit: Permit::PoseFloat,
                path: path.to_owned(),
            });
        }
        (JsonView::Text(_), JsonView::Text(_))
            if path.starts_with(".failures[")
                && want.to_json().map_err(|e| e.to_string())?
                    != got.to_json().map_err(|e| e.to_string())? =>
        {
            let (original, actual) = reports.ok_or("diagnostic outside report")?;
            diagnostic(
                &want.to_utf8().ok_or("surrogate failure")?,
                &got.to_utf8().ok_or("surrogate failure")?,
                original,
                actual,
                budget,
            )?;
            state.summary.derived_diagnostic_differences += 1;
            state.leaves.push(Leaf {
                want: want.clone(),
                got: got.clone(),
                permit: Permit::DerivedDiagnostic,
                path: path.to_owned(),
            });
        }
        _ => {
            if want.to_json().map_err(|e| e.to_string())?
                != got.to_json().map_err(|e| e.to_string())?
            {
                return Err(format!(
                    "{path}: exact report value or classification changed"
                ));
            }
            state.leaves.push(Leaf {
                want: want.clone(),
                got: got.clone(),
                permit: Permit::Exact,
                path: path.to_owned(),
            });
        }
    }
    Ok(())
}
pub fn verify(original: &[u8], actual: &[u8]) -> Result<Summary> {
    let want = std::str::from_utf8(original).map_err(|e| e.to_string())?;
    let got = std::str::from_utf8(actual).map_err(|e| e.to_string())?;
    let a = JsonValue::parse(want).map_err(|e| e.to_string())?;
    let b = JsonValue::parse(got).map_err(|e| e.to_string())?;
    let mut state = WalkState::default();
    walk(&a, &b, "", false, budget()?, Some((&a, &b)), &mut state)?;
    substitute(want, got, &state.leaves)?;
    Ok(state.summary)
}
