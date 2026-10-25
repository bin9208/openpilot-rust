//! Selected project fields from independently captured sentry-sdk 2.55.0 events.
//! Candidate events never supply expected fields. SDK internals stay outside the
//! original comparison contract; actual transport is exercised by the controller.
use crate::seal::Result;
use openpilot_log_format::{JsonValue as J, JsonView};
use sha2::{Digest, Sha256};
#[cfg(feature = "tombstone-tools-native")]
mod runtime;
#[cfg(feature = "tombstone-tools-native")]
pub use runtime::run;

pub fn expected(index: usize) -> Result<J> {
    let bytes = include_bytes!("../tests/fixtures/sdk-receiver/original-sdk-fields.json");
    if format!("{:x}", Sha256::digest(bytes))
        != "0771460e1aa173f3e890c9f9131a12372b674187e3906bd035904866719a7fca"
    {
        return Err("original external SDK capture changed".into());
    }
    let source = J::parse(std::str::from_utf8(bytes)?)?;
    let JsonView::Array(rows) = field(&source, "cases")?.view() else {
        return Err("original SDK cases are not an array".into());
    };
    if rows.len() != 14 {
        return Err("original SDK case count changed".into());
    }
    let row = rows.get(index).ok_or("unknown original SDK case")?;
    if !field(row, "platform")?.text_eq("python")
        || !field(&field(row, "sdk")?, "name")?.text_eq("sentry.python")
    {
        return Err("expected event was not captured from the original SDK".into());
    }
    field(row, "expected")
}
fn field(value: &J, key: &str) -> Result<J> {
    value
        .get(key)
        .ok_or_else(|| format!("missing SDK field {key}").into())
}
fn equal(a: &J, b: &J) -> bool {
    match (a.view(), b.view()) {
        (JsonView::Null, JsonView::Null) => true,
        (JsonView::Bool(a), JsonView::Bool(b)) => a == b,
        (JsonView::Integer(a), JsonView::Integer(b)) => a == b,
        (JsonView::Float(a), JsonView::Float(b)) => a == b,
        (JsonView::Text(a), JsonView::Text(b)) => a == b,
        (JsonView::Array(a), JsonView::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| equal(a, b))
        }
        (JsonView::Object(a), JsonView::Object(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, a)| {
                    b.iter()
                        .find(|(other, _)| other == key)
                        .is_some_and(|(_, b)| equal(a, b))
                })
        }
        _ => false,
    }
}
/// The original thirteen SDK controls compare seven project-owned fields,
/// including lossless JSON types and external SDK truncation annotations.
pub fn verify(index: usize, actual: &J, auth: Option<&str>) -> Result<()> {
    if index >= 13 {
        return Err("dirty metadata requires current source calls".into());
    }
    let source = expected(index)?;
    let null = J::parse("null")?;
    for key in [
        "message",
        "tags",
        "user",
        "extra",
        "release",
        "environment",
        "_meta",
    ] {
        if !equal(
            &actual.get(key).unwrap_or_else(|| null.clone()),
            &field(&source, key)?,
        ) {
            return Err(
                format!("SDK case {index}: {key} differs from original external SDK").into(),
            );
        }
    }
    if !matches!(
        field(&field(actual, "tags")?, "dirty")?.view(),
        JsonView::Bool(_)
    ) || !field(actual, "platform")?.text_eq("native")
        || !field(&field(actual, "sdk")?, "name")?.text_eq("sentry.rust")
        || !auth.is_some_and(|value| value.contains("sentry_key=fixture"))
    {
        return Err("native SDK identity, dirty boolean or fixture authentication differs".into());
    }
    Ok(())
}
/// Only the actual independent source Git commit is rebound; all other dirty
/// tags, including the persisted truncated unusual value, remain source captures.
pub fn verify_dirty(actual: &J, source_calls: &J) -> Result<()> {
    let JsonView::Array(calls) = source_calls.view() else {
        return Err("source SDK calls are not an array".into());
    };
    let commit = calls
        .iter()
        .rev()
        .find_map(|call| {
            if !call.get("op")?.text_eq("set_tag") {
                return None;
            }
            let fields = call.get("fields")?;
            if !fields.get("key")?.text_eq("commit") {
                return None;
            }
            fields.get("value_json")?.to_utf8()
        })
        .ok_or("independent source did not set the Git commit tag")?;
    let commit = J::parse(&commit)?;
    let tags = field(&expected(13)?, "tags")?;
    let JsonView::Object(fields) = tags.view() else {
        return Err("original SDK tags are not an object".into());
    };
    let mut members = Vec::new();
    for (key, value) in fields {
        let key = J::codepoints(key.to_vec()).ok_or("invalid original tag name")?;
        let value = if key.text_eq("commit") {
            &commit
        } else {
            &value
        };
        members.push(format!("{}:{}", key.to_json()?, value.to_json()?));
    }
    let expected = J::parse(&format!("{{{}}}", members.join(",")))?;
    let actual = field(actual, "tags")?;
    if !matches!(field(&actual, "dirty")?.view(), JsonView::Bool(true))
        || !equal(&actual, &expected)
    {
        return Err("dirty metadata SDK tags differ from independent source".into());
    }
    Ok(())
}
