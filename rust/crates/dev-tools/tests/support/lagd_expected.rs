//! Immutable original-only synthetic boundary captures and independent comparison.
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::PathBuf};

pub const BOUNDARY_SHA: &str = "56ff47d710bccb32fb0f32f80b66a9fd448ae523fdd90f3df6fea17945bd65f4";
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lagd-tools")
}
fn sha(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
pub fn original(name: &str) -> Value {
    let raw = fs::read(root().join("boundary-manifest.json")).unwrap();
    assert_eq!(sha(&raw), BOUNDARY_SHA);
    let manifest: Value = serde_json::from_slice(&raw).unwrap();
    let manifest = if manifest["files"].get(name).is_some() {
        manifest
    } else {
        let (filename, expected) = if name == "loop-admission-original.json.gz" {
            (
                "loop-admission-manifest.json",
                "889652e4b797c17a646dddb64db4c084ded90c35d6438d2b6cdf10a0129456c3",
            )
        } else if matches!(
            name,
            "wire-neighbors-original.json.gz" | "loop-neighbors-original.json.gz"
        ) {
            (
                "reference-manifest.json",
                "1dd8e938076f564aa6752cf4f0649aae137dba057b94e54b8eb647d919286162",
            )
        } else {
            (
                "manifest.json",
                "26ef2f7cb7e295ff4315e121f9cfd63c60100aa51e4c0727f54623fb3bef7ab0",
            )
        };
        let bytes = fs::read(root().join(filename)).unwrap();
        assert_eq!(sha(&bytes), expected);
        serde_json::from_slice::<Value>(&bytes).unwrap()
    };
    let raw = fs::read(root().join(name)).unwrap();
    assert_eq!(sha(&raw), manifest["files"][name]["sha256"], "{name}");
    let raw = if name.ends_with(".gz") {
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        if let Some(expected) = manifest["files"][name].get("decoded_sha256") {
            assert_eq!(sha(&decoded), expected.as_str().unwrap(), "{name}");
        }
        decoded
    } else {
        raw
    };
    serde_json::from_slice(&raw).unwrap()
}
fn scalar(value: &JsonValue) -> Option<f64> {
    match value.view() {
        JsonView::Integer(text) => text.parse().ok(),
        JsonView::Float(value) => Some(value),
        JsonView::Bool(value) => Some(if value { 1. } else { 0. }),
        _ => None,
    }
}
/// Same fixed numerical budgets and exact discrete classification as the original.
/// Ordered objects are required for serialized report artifacts, not dict equality.
pub fn compare(
    expected: &JsonValue,
    actual: &JsonValue,
    budget: (f64, f64),
    ordered: bool,
    path: &str,
) -> Result<(), String> {
    match expected.view() {
        JsonView::Object(want) => {
            let JsonView::Object(got) = actual.view() else {
                return Err(format!("{path}: not an object"));
            };
            if want.len() != got.len() {
                return Err(format!("{path}: object key count"));
            }
            if ordered
                && !want
                    .iter()
                    .map(|(key, _)| *key)
                    .eq(got.iter().map(|(key, _)| *key))
            {
                return Err(format!("{path}: report key order"));
            }
            for (key, expected) in want {
                let (_, actual) = got
                    .iter()
                    .find(|(candidate, _)| *candidate == key)
                    .ok_or_else(|| format!("{path}: missing object key"))?;
                let key: String = key
                    .iter()
                    .copied()
                    .map(|v| char::from_u32(v).unwrap_or('\u{fffd}'))
                    .collect();
                compare(&expected, actual, budget, ordered, &format!("{path}.{key}"))?;
            }
        }
        JsonView::Array(want) => {
            let JsonView::Array(got) = actual.view() else {
                return Err(format!("{path}: not an array"));
            };
            if want.len() != got.len() {
                return Err(format!("{path}: array length"));
            }
            for (index, (want, got)) in want.iter().zip(got).enumerate() {
                compare(want, &got, budget, ordered, &format!("{path}[{index}]"))?;
            }
        }
        JsonView::Float(want) => {
            let got = scalar(actual).ok_or_else(|| format!("{path}: not a number"))?;
            if want.is_nan() && got.is_nan() {
                return Ok(());
            }
            if !want.is_finite() || !got.is_finite() {
                if want == got {
                    return Ok(());
                }
                return Err(format!("{path}: finite/nonfinite classification"));
            }
            let tolerance = budget
                .0
                .max(budget.1 * want.abs())
                .max(budget.1 * got.abs());
            if (want - got).abs() > tolerance {
                return Err(format!(
                    "{path}: source={want:?} actual={got:?} budget={budget:?}"
                ));
            }
        }
        JsonView::Integer(_) | JsonView::Bool(_) => {
            if scalar(expected) != scalar(actual) {
                return Err(format!("{path}: exact discrete number"));
            }
        }
        JsonView::Text(want) => {
            if !matches!(actual.view(), JsonView::Text(got) if want == got) {
                return Err(format!("{path}: text/classification"));
            }
        }
        JsonView::Null => {
            if !matches!(actual.view(), JsonView::Null) {
                return Err(format!("{path}: null"));
            }
        }
    }
    Ok(())
}
