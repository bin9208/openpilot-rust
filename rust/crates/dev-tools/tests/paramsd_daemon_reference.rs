//! Decode actual native controller packets independently of its transport
//! implementation and compare every row to the unchanged Python source trace.
#![cfg(feature = "paramsd-daemon-native")]
use capnp::{dynamic_struct, dynamic_value::Reader as Dynamic};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn required(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")));
    assert!(path.is_absolute(), "{name} must be absolute");
    path
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn sha(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

// The source uses pycapnp Event.to_dict(): active union first, all scalar
// defaults, and only present pointer fields. This visitor is independent of
// the paramsd daemon, peer, and native controller implementations.
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
        Dynamic::Data(_) => return Err("source Event.to_dict data field cannot be JSON".into()),
        Dynamic::AnyPointer(_) | Dynamic::Capability(_) => {
            return Err("unsupported Cereal pointer".into())
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
fn event_dict(bytes: &[u8]) -> Result<Value> {
    let message = capnp::serialize::read_message(
        &mut Cursor::new(bytes),
        capnp::message::ReaderOptions::new(),
    )?;
    let mut event = project(message.get_root::<event::Reader<'_>>()?.into())?;
    event
        .as_object_mut()
        .ok_or("Event is not an object")?
        .remove("logMonoTime");
    Ok(event)
}

// Mirrors the unchanged check_paramsd.compare path policy. numpy.isclose uses
// abs(a-b) <= atol + rtol*abs(b), with the tighter debugFilterState exception.
fn compare(expected: &Value, actual: &Value, path: &str) {
    match expected {
        Value::Object(fields) => {
            let found = actual
                .as_object()
                .unwrap_or_else(|| panic!("{path}: expected object"));
            assert_eq!(
                fields.keys().collect::<Vec<_>>(),
                found.keys().collect::<Vec<_>>(),
                "{path}: keys"
            );
            for (key, value) in fields {
                compare(value, &found[key], &format!("{path}.{key}"));
            }
        }
        Value::Array(items) => {
            let found = actual
                .as_array()
                .unwrap_or_else(|| panic!("{path}: expected array"));
            assert_eq!(items.len(), found.len(), "{path}: length");
            for (index, (left, right)) in items.iter().zip(found).enumerate() {
                compare(left, right, &format!("{path}[{index}]"));
            }
        }
        Value::Number(number) if number.is_f64() => {
            let left = number.as_f64().unwrap();
            let right = actual
                .as_f64()
                .unwrap_or_else(|| panic!("{path}: expected number"));
            let tolerance = if path.contains(".event.") && !path.contains(".debugFilterState.") {
                2e-6
            } else {
                1e-9
            };
            assert!(
                left.is_finite()
                    && right.is_finite()
                    && (left - right).abs() <= tolerance + tolerance * right.abs(),
                "{path}: {left} != {right}"
            );
        }
        _ => assert_eq!(expected, actual, "{path}"),
    }
}

#[test]
fn full_native_controller_matches_original_source_trace() {
    let original = required("PARAMSD_ORIGINAL_OUTPUT");
    let native = required("PARAMSD_NATIVE_OUTPUT");
    let daemon = required("PARAMSD_NATIVE_DAEMON");
    let hook = required("PARAMSD_NATIVE_HOOK");
    let pins = read(&native.join("source-identities.json"));
    assert_eq!(pins["daemon"], sha(&daemon));
    assert_eq!(pins["phase_hook"], sha(&hook));
    assert_eq!(pins["unchanged_after_run"], true);
    let top = read(&native.join("result.json"));
    assert_eq!(top["pass"], true);
    assert_eq!(top["continuous_publications"], 1202);
    assert_eq!(top["restart_publications"], 8);
    for (name, count) in [("continuous", 1202), ("restart", 8)] {
        let source = original.join(name);
        let candidate = native.join(name);
        let input_key = format!("{name}_input");
        assert_eq!(pins[input_key.as_str()], sha(&source.join("input.json")));
        let source_rows = read(&source.join("source.json"))["rows"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(source_rows.len(), count + 1);
        let raw = read(&candidate.join("packets.json"));
        let packets = raw.as_array().unwrap();
        assert_eq!(packets.len(), count);
        let journal = fs::read_to_string(candidate.join("packets.jsonl")).unwrap();
        let journal_rows = journal.lines().collect::<Vec<_>>();
        assert_eq!(journal_rows.len(), count);
        let original_result = read(&source.join("results.json"));
        let native_result = read(&candidate.join("result.json"));
        assert_eq!(original_result["pass"], true);
        assert_eq!(original_result["source_equal"], true);
        for (key, expected) in [
            ("publications", json!(count)),
            ("gps_service", original_result["gps_service"].clone()),
            ("signal", original_result["signal"].clone()),
            ("exit", original_result["exit"].clone()),
            ("startup_poll", original_result["startup_poll"].clone()),
        ] {
            assert_eq!(native_result[key], expected, "{name}: {key}");
        }
        assert_eq!(native_result["pass"], true);
        assert_eq!(native_result["no_python_runtime"], true);
        assert_eq!(
            fs::read(source.join("gps.json")).unwrap(),
            fs::read(candidate.join("gps.json")).unwrap()
        );
        compare(
            &event_dict(&fs::read(source.join("saved.bin")).unwrap()).unwrap(),
            &event_dict(&fs::read(candidate.join("saved.bin")).unwrap()).unwrap(),
            "daemon.event.saved",
        );
        if name == "continuous" {
            assert_eq!(native_result["persisted_packet_index"], 1199);
        }
        for (index, (reference, packet)) in source_rows[1..].iter().zip(packets).enumerate() {
            let raw: Vec<u8> = serde_json::from_value(packet.clone()).unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(journal_rows[index]).unwrap(),
                *packet,
                "{name}: raw packet {index}"
            );
            let mut expected = reference["event"].clone();
            expected.as_object_mut().unwrap().remove("logMonoTime");
            let actual = event_dict(&raw).unwrap();
            compare(&expected, &actual, &format!("daemon.event.row{index}"));
        }
    }
    for root in [&original, &native] {
        assert_eq!(
            fs::read(root.join("continuous/saved.bin")).unwrap(),
            fs::read(root.join("restart/saved.bin")).unwrap(),
            "{} did not retain its own saved bytes through restart",
            root.display()
        );
    }
}
