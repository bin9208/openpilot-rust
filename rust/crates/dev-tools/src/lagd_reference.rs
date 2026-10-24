//! Independent translation of the original lagd QA source extraction.
//!
//! Expected values are computed from the caller's input, never loaded from the
//! captured output corpus or delegated to the candidate lagd/calibration crates.
//! Only the source NumPy revision's external PocketFFT kernel is shared.
use crate::seal::Result;
use num_bigint::BigInt;
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

mod admission;
mod blocks;
pub(crate) mod numeric;
mod pose;

/// Only these exclusive QA sources are retired; all other provenance remains live.
pub fn retired_source(name: &str) -> bool {
    matches!(
        name,
        "rust/tools/check_lagd_numeric.py"
            | "rust/tools/check_lagd_loop.py"
            | "rust/tools/check_lagd_cache.py"
            | "rust/tools/check_lagd_packet.py"
            | "rust/tools/check_lagd_daemon.py"
            | "rust/tools/check_lagd_params_io.py"
            | "rust/tools/lagd_reference.py"
            | "rust/tools/lagd_loop_reference.py"
            | "rust/tools/lagd_frames.py"
    )
}
/// Check every present original and every mandatory runtime/schema/kernel input.
pub fn sources(root: &Path) -> Result<()> {
    let bytes = include_bytes!("../tests/fixtures/lagd-tools/source-pins.json");
    if format!("{:x}", Sha256::digest(bytes))
        != "b8205f241d7147eeee534f4903ebbc39fefe5521d75ba0ccf8e5725275570fe1"
    {
        return Err("lagd original source manifest changed".into());
    }
    let manifest: Value = serde_json::from_slice(bytes)?;
    let pins = manifest["sources"]
        .as_object()
        .ok_or("missing lagd source pins")?;
    if pins.len() != 25
        || manifest["python"] != "3.12.15"
        || manifest["numpy"] != "2.5.3"
        || manifest["numpy_commit"] != "dd88c0c19b54ad9ed3533224221285bf0873249a"
        || manifest["pycapnp"] != "2.1.0"
    {
        return Err("lagd original dependency provenance changed".into());
    }
    for (name, expected) in pins {
        let path = root.join(name);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && retired_source(name) => {
                // A dangling restored symlink is present, not a retired absence.
                match fs::symlink_metadata(&path) {
                    Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => continue,
                    _ => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        };
        if format!("{:x}", Sha256::digest(bytes))
            != expected.as_str().ok_or("invalid lagd source digest")?
        {
            return Err(format!("lagd source changed: {name}").into());
        }
    }
    Ok(())
}

fn field(input: &JsonValue, name: &str) -> Result<JsonValue> {
    input
        .get(name)
        .ok_or_else(|| format!("missing lagd numeric key: {name}").into())
}
fn array(input: &JsonValue) -> Result<Vec<JsonValue>> {
    match input.view() {
        JsonView::Array(values) => Ok(values),
        _ => Err("lagd numeric input must be a sequence".into()),
    }
}
fn number(input: &JsonValue) -> Result<f64> {
    match input.view() {
        JsonView::Float(value) => Ok(value),
        JsonView::Bool(value) => Ok(if value { 1. } else { 0. }),
        JsonView::Null => Ok(f64::NAN), // np.asarray(None, dtype=float64)
        JsonView::Integer(text) => {
            let value: f64 = text.parse()?;
            if !value.is_finite() {
                return Err("int too large to convert to float".into());
            }
            Ok(value)
        }
        JsonView::Text(_) => {
            let text = input.to_utf8().ok_or("surrogate in numeric text")?;
            openpilot_runtime_core::python_float::parse(text.trim_end_matches('\0'))
                .ok_or_else(|| "could not convert string to float".into())
        }
        _ => Err("lagd numeric value is not a scalar".into()),
    }
}
fn integer(input: &JsonValue) -> Result<BigInt> {
    match input.view() {
        JsonView::Integer(text) => Ok(text.parse()?),
        JsonView::Bool(value) => Ok(BigInt::from(u8::from(value))),
        _ => Err("lagd numeric index must be an integer".into()),
    }
}
fn index(input: &JsonValue) -> Result<usize> {
    Ok(integer(input)?.to_string().parse()?)
}
fn signed(input: &JsonValue) -> Result<i64> {
    Ok(integer(input)?.to_string().parse()?)
}
fn numbers(input: &JsonValue) -> Result<Vec<f64>> {
    array(input)?.iter().map(number).collect()
}
fn normalized(value: f64) -> Value {
    if value.is_nan() {
        json!("nan")
    } else if value == f64::INFINITY {
        json!("inf")
    } else if value == f64::NEG_INFINITY {
        json!("-inf")
    } else {
        json!(value)
    }
}
fn normalized_array(values: &[f64]) -> Value {
    values.iter().copied().map(normalized).collect()
}
fn output(value: Value) -> Result<JsonValue> {
    Ok(JsonValue::parse(&serde_json::to_string(&value)?)?)
}

/// Evaluate the seven dynamic operations exposed by original lagd_reference.numeric.
pub fn numeric(input: &JsonValue) -> Result<JsonValue> {
    let kind = field(input, "kind")?
        .to_utf8()
        .ok_or("lagd numeric kind must be text")?;
    if kind == "padding" {
        let values = array(&field(input, "values")?)?;
        let result = values
            .iter()
            .map(admission::padding)
            .collect::<Result<Vec<_>>>()?;
        return Ok(JsonValue::parse(&format!(
            "[{}]",
            result
                .iter()
                .map(JsonValue::to_json)
                .collect::<std::result::Result<Vec<_>, _>>()?
                .join(",")
        ))?);
    }
    let value = match kind.as_str() {
        "peak" => return admission::peak(&field(input, "values")?, &field(input, "index")?),
        "smooth" => {
            let (product, mask) =
                admission::smoothing(&field(input, "values")?, &field(input, "mask")?)?;
            normalized_array(&numeric::smooth(
                &product,
                &mask,
                index(&field(input, "k")?)?,
                admission::arithmetic(&field(input, "sigma")?)?,
            )?)
        }
        "correlate" | "delay" => {
            let (expected, actual, mask) = admission::correlation(
                &field(input, "expected")?,
                &field(input, "actual")?,
                &field(input, "mask")?,
            )?;
            if kind == "correlate" {
                let n = field(input, "n")?;
                let count = match n.view() {
                    JsonView::Null => actual.len(),
                    JsonView::Integer(_) => index(&n)?,
                    _ => return Err("TypeError: FFT length must be an integer".into()),
                };
                normalized_array(&numeric::correlate(&expected, &actual, &mask, count)?)
            } else {
                numeric::delay(
                    &expected,
                    &actual,
                    &mask,
                    admission::arithmetic(&field(input, "dt")?)?,
                    admission::arithmetic(&field(input, "min")?)?,
                    admission::arithmetic(&field(input, "max")?)?,
                )?
            }
        }
        "blocks" => {
            let mut blocks = blocks::Blocks::new(
                index(&field(input, "count")?)?,
                field(input, "size")?,
                signed(&field(input, "valid")?)?,
                field(input, "initial")?,
            )?;
            let mut rows = vec![blocks.snapshot()?];
            for value in array(&field(input, "updates")?)? {
                if !matches!(value.view(), JsonView::Null) {
                    blocks.update(admission::arithmetic(&value)?)?;
                }
                rows.push(blocks.snapshot()?);
            }
            return Ok(JsonValue::parse(&format!(
                "[{}]",
                rows.iter()
                    .map(JsonValue::to_json)
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .join(",")
            ))?);
        }
        "pose" => pose::evaluate(input)?,
        _ => return Err(format!("unknown lagd numeric kind: {kind}").into()),
    };
    output(value)
}
