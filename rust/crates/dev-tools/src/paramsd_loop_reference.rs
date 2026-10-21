//! Independent, sealed original observation for the paramsd loop's 39 cases.
//! The caller passes the actual raw output of `paramsd_loop`, not its converted JSON.
use bzip2::read::BzDecoder;
use capnp::{dynamic_struct, dynamic_value::Reader as Dynamic};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const SOURCE: &[u8] = include_bytes!("../tests/fixtures/paramsd-loop-original/source.json.bz2");
const INPUT: &[u8] = include_bytes!("../tests/fixtures/paramsd-loop-original/input.json.bz2");
const SOURCE_SHA: &str = "23d374139b2430bd8da0eb800c0a8a91900c8d08c0df90373ba2f4d7419ae22a";
const SOURCE_PACKED_SHA: &str = "96b1a6dae1169fbdb39bd0cee6cce2fc67b40ea81fb9af7b2b7f392a3f99a85d";
const INPUT_SHA: &str = "4f446bc98c9d303d292f487c9ebe36bc8721436a30627dfca55baabae4c6811d";
const INPUT_PACKED_SHA: &str = "6a5641f18914cbf452f027ed33daf64a372a966ebc89ae2a2e859a4de384f818";
const NEGATIVE_INFINITY: &str = "__sealed_original_negative_infinity_f64__";

#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub cases: usize,
    pub frames: usize,
    pub saves: usize,
    pub gps_writes: usize,
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn unpack(
    bytes: &[u8],
    packed_sha: &str,
    raw_sha: &str,
    limit: u64,
    source: bool,
) -> Result<Value> {
    if sha(bytes) != packed_sha {
        return Err("sealed paramsd loop fixture compression hash changed".into());
    }
    let mut raw = Vec::new();
    BzDecoder::new(bytes)
        .take(limit + 1)
        .read_to_end(&mut raw)?;
    if raw.len() as u64 > limit || sha(&raw) != raw_sha {
        return Err("sealed paramsd loop fixture bytes changed or exceed limit".into());
    }
    if source {
        // Python json.dump emitted exactly one nonstandard token, the
        // migration-13 initial offset -Infinity. The independently pinned
        // raw SHA is checked above. Translate that one token into a private
        // sentinel so serde_json can parse the rest without losing its sign.
        let mut text = String::from_utf8(raw)?;
        if text.contains(NEGATIVE_INFINITY) || text.matches("-Infinity").count() != 1 {
            return Err("unexpected original nonfinite token inventory".into());
        }
        text = text.replacen("-Infinity", &format!("\"{NEGATIVE_INFINITY}\""), 1);
        let value: Value = serde_json::from_str(&text)?;
        if value[13]["name"] != "migration-13"
            || value[13]["init"]["initial"][2] != NEGATIVE_INFINITY
        {
            return Err("original nonfinite token moved".into());
        }
        Ok(value)
    } else {
        Ok(serde_json::from_slice(&raw)?)
    }
}
fn original() -> Result<(Value, Value)> {
    Ok((
        unpack(SOURCE, SOURCE_PACKED_SHA, SOURCE_SHA, 8_000_000, true)?,
        sealed_input()?,
    ))
}
fn sealed_input() -> Result<Value> {
    unpack(INPUT, INPUT_PACKED_SHA, INPUT_SHA, 4_500_000, false)
}

/// Require the fresh native-generated request to match every independently
/// captured original case, metadata field, ordered frame, and packet byte.
/// Invoke before the native loop child receives the request or writes success.
pub fn verify_input(request: &Value) -> Result<()> {
    if request != &sealed_input()? {
        return Err("fresh paramsd loop request differs from sealed original input".into());
    }
    Ok(())
}

// Match pycapnp Event.to_dict(): active union, scalar defaults, and present
// pointer fields. The packet is decoded here, not by the native loop program.
fn project(value: Dynamic<'_>) -> Result<Value> {
    Ok(match value {
        Dynamic::Void => Value::Null,
        Dynamic::Bool(v) => json!(v),
        Dynamic::Int8(v) => json!(v),
        Dynamic::Int16(v) => json!(v),
        Dynamic::Int32(v) => json!(v),
        Dynamic::Int64(v) => json!(v),
        Dynamic::UInt8(v) => json!(v),
        Dynamic::UInt16(v) => json!(v),
        Dynamic::UInt32(v) => json!(v),
        Dynamic::UInt64(v) => json!(v),
        Dynamic::Float32(v) => json!(f64::from(v)),
        Dynamic::Float64(v) => json!(v),
        Dynamic::Text(v) => json!(v.to_str()?),
        Dynamic::Enum(v) => json!(v
            .get_enumerant()?
            .ok_or("unknown cereal enum")?
            .get_proto()
            .get_name()?
            .to_str()?),
        Dynamic::List(v) => {
            Value::Array(v.iter().map(|item| project(item?)).collect::<Result<_>>()?)
        }
        Dynamic::Struct(v) => Value::Object(structure(v)?),
        Dynamic::Data(_) | Dynamic::AnyPointer(_) | Dynamic::Capability(_) => {
            return Err("unsupported Cereal Event pointer".into());
        }
    })
}
fn structure(value: dynamic_struct::Reader<'_>) -> Result<Map<String, Value>> {
    let mut result = Map::new();
    if let Some(field) = value.which()? {
        result.insert(
            field.get_proto().get_name()?.to_str()?.into(),
            project(value.get(field)?)?,
        );
    } else if !value.get_schema().get_union_fields()?.is_empty() {
        return Err("unknown Cereal union discriminant".into());
    }
    for field in value.get_schema().get_non_union_fields()? {
        if value.has(field)? {
            result.insert(
                field.get_proto().get_name()?.to_str()?.into(),
                project(value.get(field)?)?,
            );
        }
    }
    Ok(result)
}
fn event_dict(value: &Value) -> Result<Value> {
    let bytes: Vec<u8> = serde_json::from_value(value.clone())?;
    let message = capnp::serialize::read_message(
        &mut Cursor::new(&bytes),
        capnp::message::ReaderOptions::new(),
    )?;
    project(message.get_root::<event::Reader<'_>>()?.into())
}
fn floats(bits: &Value) -> Result<Value> {
    let values = bits.as_array().ok_or("native float bits array")?;
    values
        .iter()
        .map(|value| {
            let bits = value.as_u64().ok_or("native float bits value")?;
            let float = f64::from_bits(bits);
            if float == f64::NEG_INFINITY {
                return Ok(json!(NEGATIVE_INFINITY));
            }
            if !float.is_finite() {
                return Err("nonfinite native float".into());
            }
            Ok(json!(float))
        })
        .collect::<Result<Vec<_>>>()
        .map(Value::Array)
}
fn normalize_logs(record: &mut Value) -> Result<()> {
    for entry in record["logs"].as_array_mut().ok_or("logs array")? {
        let fields = entry.as_array_mut().ok_or("log pair")?;
        if fields.len() != 2 {
            return Err("log pair length".into());
        }
        if fields[0] == "error" {
            let message = fields[1].as_str().ok_or("error log text")?;
            fields[1] = json!(message.split(':').next().unwrap_or(""));
        }
    }
    Ok(())
}
fn normalize_source(source: &mut Value) -> Result<()> {
    for case in source.as_array_mut().ok_or("original cases array")? {
        normalize_logs(&mut case["init"])?;
        for row in case["rows"].as_array_mut().ok_or("original rows array")? {
            normalize_logs(row)?;
        }
        for operation in case["init"]["operations"]
            .as_array_mut()
            .ok_or("original operations")?
        {
            let action = operation.as_array_mut().ok_or("original operation")?;
            if action.len() != 3 {
                return Err("original operation length".into());
            }
            if action[0] == "put" {
                action[2] = event_dict(&action[2])?;
            }
        }
    }
    Ok(())
}
fn convert_native(raw: &mut Value) -> Result<()> {
    for case in raw.as_array_mut().ok_or("native cases array")? {
        let init = case["init"].as_object_mut().ok_or("native init")?;
        let initial = init.remove("initial_bits").ok_or("native initial bits")?;
        init.insert("initial".into(), floats(&initial)?);
        let covariance = init
            .remove("covariance_bits")
            .ok_or("native covariance bits")?;
        init.insert(
            "covariance".into(),
            if covariance.is_null() {
                Value::Null
            } else {
                floats(&covariance)?
            },
        );
        normalize_logs(&mut case["init"])?;
        for row in case["rows"].as_array_mut().ok_or("native rows array")? {
            let fields = row.as_object_mut().ok_or("native row")?;
            let x = fields.remove("x_bits").ok_or("native x bits")?;
            let p = fields.remove("p_bits").ok_or("native p bits")?;
            let packet = fields.remove("packet").ok_or("native packet")?;
            fields.insert("x".into(), floats(&x)?);
            fields.insert("p".into(), floats(&p)?);
            fields.insert(
                "event".into(),
                if packet.is_null() {
                    Value::Null
                } else {
                    event_dict(&packet)?
                },
            );
            normalize_logs(row)?;
        }
        for operation in case["init"]["operations"]
            .as_array_mut()
            .ok_or("native operations")?
        {
            let action = operation.as_array_mut().ok_or("native operation")?;
            if action.len() != 3 {
                return Err("native operation length".into());
            }
            if action[0] == "put" {
                action[2] = event_dict(&action[2])?;
            }
        }
    }
    Ok(())
}

// check_paramsd.compare: exact structure and nonfloat values, numpy.isclose
// for floats (equal_nan=True), 2e-6 inside Event except debugFilterState,
// 1e-9 elsewhere. The one original -Infinity is an exact f64-bit sentinel.
fn compare(expected: &Value, actual: &Value, path: &str) -> Result<()> {
    match expected {
        Value::Object(fields) => {
            let found = actual
                .as_object()
                .ok_or_else(|| format!("{path}: expected object"))?;
            if fields.keys().collect::<Vec<_>>() != found.keys().collect::<Vec<_>>() {
                return Err(format!("{path}: object fields differ").into());
            }
            for (key, value) in fields {
                compare(value, &found[key], &format!("{path}.{key}"))?;
            }
        }
        Value::Array(items) => {
            let found = actual
                .as_array()
                .ok_or_else(|| format!("{path}: expected array"))?;
            if items.len() != found.len() {
                return Err(format!("{path}: array length differs").into());
            }
            for (index, (left, right)) in items.iter().zip(found).enumerate() {
                compare(left, right, &format!("{path}[{index}]"))?;
            }
        }
        Value::Number(number) if number.is_f64() => {
            let left = number.as_f64().ok_or("original float")?;
            let right = actual
                .as_f64()
                .ok_or_else(|| format!("{path}: expected float"))?;
            let tolerance = if path.contains(".event.") && !path.contains(".debugFilterState.") {
                2e-6
            } else {
                1e-9
            };
            if !(left.is_finite()
                && right.is_finite()
                && (left - right).abs() <= tolerance + tolerance * right.abs())
            {
                return Err(format!("{path}: {left} != {right}").into());
            }
        }
        _ if expected != actual => return Err(format!("{path}: value differs").into()),
        _ => {}
    }
    Ok(())
}
fn compare_values(mut source: Value, input: Value, actual: &Value) -> Result<Summary> {
    let cases = source.as_array().ok_or("original cases")?;
    let inputs = input["cases"].as_array().ok_or("original input cases")?;
    if cases.len() != 39 || inputs.len() != 39 {
        return Err("sealed original case count differs".into());
    }
    let mut frames = 0;
    let mut saves = 0;
    let mut gps_writes = 0;
    for (case, request) in cases.iter().zip(inputs) {
        if case["name"] != request["name"] {
            return Err("sealed original case order differs from input".into());
        }
        let rows = case["rows"].as_array().ok_or("original rows")?;
        let request_frames = request["frames"].as_array().ok_or("original frames")?;
        if rows.len() != request_frames.len() {
            return Err("sealed original row count differs from input".into());
        }
        frames += rows.len();
        saves += rows.iter().filter(|row| row["cache"] == true).count();
        gps_writes += rows.iter().filter(|row| !row["gps"].is_null()).count();
    }
    if frames != 2420 || saves != 4 || gps_writes != 216 {
        return Err("sealed original loop coverage differs".into());
    }
    normalize_source(&mut source)?;
    compare(&source, actual, "loop")?;
    Ok(Summary {
        cases: 39,
        frames,
        saves,
        gps_writes,
    })
}

/// Compare all actual raw loop results with source-only historical observations.
/// No native output is stored as an expected fixture.
pub fn compare_file(path: &Path) -> Result<Summary> {
    let (source, input) = original()?;
    let file = fs::File::open(path)?;
    let mut raw = Vec::new();
    file.take(32_000_001).read_to_end(&mut raw)?;
    if raw.len() > 32_000_000 {
        return Err("native paramsd loop output exceeds bound".into());
    }
    let mut native: Value = serde_json::from_slice(&raw)?;
    convert_native(&mut native)?;
    compare_values(source, input, &native)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_request_requires_exact_original_packets_order_and_case_names() -> Result<()> {
        let request = sealed_input()?;
        verify_input(&request)?;

        let mut changed = request.clone();
        let packet = changed["cases"][37]["frames"][0]["messages"][0]
            .as_array_mut()
            .ok_or("first packet")?;
        packet[0] = json!(packet[0].as_u64().ok_or("first packet byte")? ^ 1);
        assert!(verify_input(&changed).is_err());

        let mut changed = request.clone();
        changed["cases"][37]["frames"][0]["messages"]
            .as_array_mut()
            .ok_or("first frame messages")?
            .swap(0, 1);
        assert!(verify_input(&changed).is_err());

        let mut changed = request;
        changed["cases"][0]["name"] = json!("different-migration");
        assert!(verify_input(&changed).is_err());
        Ok(())
    }

    #[test]
    fn sealed_corpus_and_independent_corruptions() -> Result<()> {
        let (source, input) = original()?;
        let mut actual = source.clone();
        normalize_source(&mut actual)?;
        assert_eq!(
            compare_values(source.clone(), input.clone(), &actual)?,
            Summary {
                cases: 39,
                frames: 2420,
                saves: 4,
                gps_writes: 216,
            }
        );

        let corruptions: [fn(&mut Value); 7] = [
            |value: &mut Value| value[0]["name"] = json!("other-case"),
            |value: &mut Value| value[8]["init"]["operations"][0][1] = json!("OtherParam"),
            |value: &mut Value| value[37]["rows"][0]["cache"] = json!(false),
            |value: &mut Value| value[37]["rows"][0]["gps"] = json!([1, 2]),
            |value: &mut Value| value[37]["rows"][100]["event"]["valid"] = json!(true),
            |value: &mut Value| value[38]["rows"][1209]["x"][0] = json!(1000.0),
            |value: &mut Value| value[38]["rows"][1209]["logs"] = json!([["error", "changed"]]),
        ];
        for corrupt in corruptions {
            let mut changed = actual.clone();
            corrupt(&mut changed);
            assert!(compare_values(source.clone(), input.clone(), &changed).is_err());
        }
        let mut dropped = actual.clone();
        dropped[38]["rows"].as_array_mut().ok_or("rows")?.pop();
        assert!(compare_values(source, input, &dropped).is_err());
        Ok(())
    }

    #[test]
    fn rejects_nonfinite_raw_float_and_extra_structure() -> Result<()> {
        assert!(floats(&json!([f64::NAN.to_bits()])).is_err());
        assert!(floats(&json!([f64::INFINITY.to_bits()])).is_err());
        assert_eq!(
            floats(&json!([f64::NEG_INFINITY.to_bits()]))?,
            json!([NEGATIVE_INFINITY])
        );
        let (source, input) = original()?;
        let mut actual = source.clone();
        normalize_source(&mut actual)?;
        actual[0]["unexpected"] = json!(true);
        assert!(compare_values(source, input, &actual).is_err());
        Ok(())
    }
}
