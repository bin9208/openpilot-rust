//! Compare only independently captured original values; no floating tolerance.
use super::{bytes, entries, field, Result, J, V};
use capnp::{dynamic_value::Reader as D, message::ReaderOptions, serialize};
use openpilot_cereal::car_capnp::car_params;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
};

pub fn canonical_inputs(value: &J) -> Result<Value> {
    normalize(value)
}

fn normalize(value: &J) -> Result<Value> {
    Ok(match value.view() {
        V::Null => Value::Null,
        V::Bool(v) => json!(v),
        V::Integer(v) => serde_json::from_str(v)?,
        V::Float(v) => json!({"$f64": format!("{:016x}",v.to_bits())}),
        V::Text(_) => json!(super::text(value)?),
        V::Array(rows) => Value::Array(rows.iter().map(normalize).collect::<Result<_>>()?),
        V::Object(_) => {
            let mut fields = serde_json::Map::new();
            for (key, value) in entries(value)? {
                let normalized = if key == "messages" {
                    let V::Array(rows) = value.view() else {
                        return Err("packet roster not array".into());
                    };
                    Value::Array(
                        rows.iter()
                            .map(|p| normalize(&super::project_event(&bytes(p)?)?))
                            .collect::<Result<_>>()?,
                    )
                } else if key == "CarParams" {
                    normalize(&project_car(&bytes(&value)?)?)?
                } else {
                    normalize(&value)?
                };
                fields.insert(key, normalized);
            }
            Value::Object(fields)
        }
    })
}
fn project_car(bytes: &[u8]) -> Result<J> {
    let mut input = bytes;
    let message = serialize::read_message(&mut input, ReaderOptions::new())?;
    if !input.is_empty() {
        return Err("trailing fixture CarParams bytes".into());
    }
    let value: D<'_> = message.get_root::<car_params::Reader<'_>>()?.into();
    super::wire::project(value)
}
fn result(input: &J, output: &J) -> Result<Value> {
    match super::text(&field(input, "op")?)?.as_str() {
        "parameters" => normalize(&project_car(&bytes(output)?)?),
        "event" => normalize(&super::project_event(&bytes(output)?)?),
        _ => normalize(output),
    }
}
fn compare(a: &Value, b: &Value, path: &str) -> Result<()> {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            if a.keys().collect::<Vec<_>>() != b.keys().collect::<Vec<_>>() {
                return Err(format!("full-schema keys differ at {path}").into());
            }
            for (key, v) in a {
                compare(v, &b[key], &format!("{path}.{key}"))?;
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("list length differs at {path}").into());
            }
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"))?;
            }
        }
        _ if a != b => {
            return Err(format!("exact original value differs at {path}: {a} vs {b}").into())
        }
        _ => {}
    }
    Ok(())
}
fn wire_counts(old: &J, native: &J, packet: bool) -> Result<(u64, u64)> {
    if packet {
        return Ok((1, u64::from(bytes(old)? == bytes(native)?)));
    }
    let mut total = 0;
    let mut equal = 0;
    match old.view() {
        V::Array(rows) => {
            let V::Array(other) = native.view() else {
                return Err("wire roster type changed".into());
            };
            for (a, b) in rows.iter().zip(&other) {
                let (n, e) = wire_counts(a, b, false)?;
                total += n;
                equal += e;
            }
        }
        V::Object(_) => {
            for (name, value) in entries(old)? {
                let other = field(native, &name)?;
                if name == "messages" {
                    let V::Array(a) = value.view() else {
                        return Err("messages not array".into());
                    };
                    let V::Array(b) = other.view() else {
                        return Err("native messages not array".into());
                    };
                    for (a, b) in a.iter().zip(&b) {
                        let (n, e) = wire_counts(a, b, true)?;
                        total += n;
                        equal += e;
                    }
                } else {
                    let (n, e) = wire_counts(&value, &other, name == "CarParams")?;
                    total += n;
                    equal += e;
                }
            }
        }
        _ => {}
    }
    Ok((total, equal))
}
pub fn audit_saved(proof: &Path) -> Result<()> {
    let manifest: Value = serde_json::from_slice(&fs::read(proof.join("original-manifest.json"))?)?;
    for name in [
        "requests.jsonl",
        "original.jsonl",
        "original-projection.jsonl",
    ] {
        if crate::seal::digest(&proof.join(name))?
            != manifest["files"][name]["sha256"]
                .as_str()
                .ok_or("missing original file seal")?
        {
            return Err(format!("original file changed: {name}").into());
        }
    }
    for (name, hash) in super::SOURCES {
        if manifest["sources"][name] != hash {
            return Err(format!("original source admission changed: {name}").into());
        }
    }
    let mut requests = BufReader::new(fs::File::open(proof.join("requests.jsonl"))?).lines();
    let mut original = BufReader::new(fs::File::open(proof.join("original.jsonl"))?).lines();
    let mut expected =
        BufReader::new(fs::File::open(proof.join("original-projection.jsonl"))?).lines();
    let mut actual = BufReader::new(fs::File::open(proof.join("native.jsonl"))?).lines();
    let mut projections = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(proof.join("native-projection.jsonl"))?;
    let mut count = 0;
    let mut raw_equal = 0;
    let mut packet_count = 0;
    let mut packet_bytes_equal = 0;
    for row in requests.by_ref() {
        let input = J::parse(&row?)?;
        let old = J::parse(&original.next().ok_or("missing original result")??)?;
        let expected: Value =
            serde_json::from_str(&expected.next().ok_or("missing original projection")??)?;
        let native = J::parse(&actual.next().ok_or("missing native result")??)?;
        // Check our full-schema projection against pycapnp on original wire
        // before using the same reader to inspect the native wire.
        compare(
            &expected,
            &result(&input, &old)?,
            &format!("original-projection[{count}]"),
        )?;
        let normalized = result(&input, &native)?;
        compare(&expected, &normalized, &format!("native[{count}]"))?;
        writeln!(projections, "{}", serde_json::to_string(&normalized)?)?;
        let direct = matches!(
            super::text(&field(&input, "op")?)?.as_str(),
            "event" | "parameters"
        );
        let (packets, equal) = wire_counts(&old, &native, direct)?;
        packet_count += packets;
        packet_bytes_equal += equal;
        raw_equal += usize::from(old.to_json()? == native.to_json()?);
        count += 1;
    }
    if original.next().is_some() || expected.next().is_some() || actual.next().is_some() {
        return Err("extra fixture output record".into());
    }
    if count
        != manifest["requests"]
            .as_u64()
            .ok_or("missing original request count")? as usize
    {
        return Err("original request roster changed".into());
    }
    projections.sync_all()?;
    let mut receipt = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(proof.join("comparison.json"))?;
    receipt.write_all(&serde_json::to_vec_pretty(&json!({"pass":true,"requests":count,"raw_result_records_equal":raw_equal,"packet_pairs":packet_count,"packet_pairs_byte_identical":packet_bytes_equal,"schema_and_float_bits":"exact; independently checked original pycapnp projection","expected_origin":"actual original fixture/scenario helpers","sources_retired":false,"runtime_acceptance":false}))?)?;
    receipt.sync_all()?;
    Ok(())
}
