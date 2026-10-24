//! The original comparison's type branches, fixed budgets and tuple diagnostics.
use super::repr::repr;
use crate::{
    control_tools_policy_check::json, lagd_frames, lagd_packet_reference::object, seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
#[derive(Debug)]
pub(super) struct Mismatch(pub String);
impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Mismatch {}
fn mismatch(values: &[JsonValue]) -> Result<()> {
    Err(Mismatch(format!(
        "({})",
        values.iter().map(repr).collect::<Vec<_>>().join(", ")
    ))
    .into())
}
fn tuple(path: &str, expected: &JsonValue, actual: &JsonValue) -> Result<()> {
    mismatch(&[JsonValue::text(path), expected.clone(), actual.clone()])
}
fn budget(absolute: f64, relative: f64) -> Result<JsonValue> {
    object(&[
        ("absolute", JsonValue::parse(&format!("{absolute:?}"))?),
        ("relative", JsonValue::parse(&format!("{relative:?}"))?),
    ])
}
pub(super) fn compare(
    expected: &JsonValue,
    actual: &JsonValue,
    absolute: f64,
    relative: f64,
    path: &str,
) -> Result<()> {
    match expected.view() {
        JsonView::Object(expected_values) => {
            let JsonView::Object(actual_values) = actual.view() else {
                return tuple(path, expected, actual);
            };
            if expected_values.len() != actual_values.len()
                || expected_values
                    .iter()
                    .any(|(key, _)| !actual_values.iter().any(|(other, _)| key == other))
            {
                return tuple(path, expected, actual);
            }
            for (key, value) in expected_values {
                let other = &actual_values
                    .iter()
                    .find(|(other, _)| *other == key)
                    .ok_or("dictionary key disappeared")?
                    .1;
                let key = JsonValue::codepoints(key.to_vec())
                    .ok_or("invalid key")?
                    .to_utf8()
                    .ok_or("surrogate in source comparison path")?;
                compare(&value, other, absolute, relative, &format!("{path}.{key}"))?;
            }
        }
        JsonView::Array(expected_values) => {
            let JsonView::Array(actual_values) = actual.view() else {
                return tuple(
                    path,
                    &JsonValue::parse(&expected_values.len().to_string())?,
                    actual,
                );
            };
            if expected_values.len() != actual_values.len() {
                return tuple(
                    path,
                    &JsonValue::parse(&expected_values.len().to_string())?,
                    actual,
                );
            }
            for (index, (want, got)) in expected_values.iter().zip(&actual_values).enumerate() {
                compare(want, got, absolute, relative, &format!("{path}[{index}]"))?;
            }
        }
        JsonView::Float(want) => {
            let numeric = matches!(
                actual.view(),
                JsonView::Float(_) | JsonView::Integer(_) | JsonView::Bool(_)
            );
            let got = if numeric {
                Some(lagd_frames::number(actual)?)
            } else {
                None
            };
            let close = got.is_some_and(|got| {
                want == got
                    || (!want.is_infinite()
                        && !got.is_infinite()
                        && (want - got).abs()
                            <= absolute
                                .max(relative * want.abs())
                                .max(relative * got.abs()))
            });
            if !close {
                return mismatch(&[
                    JsonValue::text(path),
                    expected.clone(),
                    actual.clone(),
                    budget(absolute, relative)?,
                ]);
            }
        }
        _ => {
            if !json::equal(expected, actual) {
                return tuple(path, expected, actual);
            }
        }
    }
    Ok(())
}
