#![cfg(feature = "proclog-reference-native")] // Parent promotes after explicit feature/test registration and API RED.
use capnp::dynamic_value::{Builder as Dynamic, Reader as Datum};
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::proclog_reference_check as checker;
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, io::Read, os::unix::ffi::OsStringExt, path::Path};

fn rows() -> Vec<Value> {
    let compressed = include_bytes!("fixtures/proclog-reference/original.json.gz");
    assert_eq!(
        format!("{:x}", Sha256::digest(compressed)),
        "36615e1d74a7d7b7e8f23eb8493a107d99a67dba4714e9a169a380ea2eb060cc"
    );
    let mut data = vec![];
    flate2::read::GzDecoder::new(compressed.as_slice())
        .read_to_end(&mut data)
        .unwrap();
    assert_eq!(data.len(), 469005);
    assert_eq!(
        format!("{:x}", Sha256::digest(&data)),
        "65c52b1c8f1bc1da50cae52b43edcd93082a33e07456ecc40b47d2a1251bfe48"
    );
    // Decode the same immutable bytes through the existing lossless parser.
    // serde_json without float_roundtrip moves 0.029999999329447746 by one ULP.
    let original = JsonValue::parse(std::str::from_utf8(&data).unwrap()).unwrap();
    let Value::Array(rows) = exact_json(&original) else {
        panic!("original rows not array")
    };
    assert_eq!(
        rows[0]["event"]["procLog"]["cpuTimes"][1]["nice"]
            .as_f64()
            .unwrap()
            .to_bits(),
        f64::from(0.03_f32).to_bits()
    );
    rows
}
fn exact_json(value: &JsonValue) -> Value {
    match value.view() {
        JsonView::Null => Value::Null,
        JsonView::Bool(v) => Value::Bool(v),
        JsonView::Integer(v) if v.starts_with('-') => Value::from(v.parse::<i64>().unwrap()),
        JsonView::Integer(v) => Value::from(v.parse::<u64>().unwrap()),
        JsonView::Float(v) => Value::from(v),
        JsonView::Text(_) => Value::String(value.to_utf8().unwrap()),
        JsonView::Array(v) => Value::Array(v.iter().map(exact_json).collect()),
        JsonView::Object(v) => Value::Object(
            v.iter()
                .map(|(key, value)| {
                    (
                        key.iter().map(|c| char::from_u32(*c).unwrap()).collect(),
                        exact_json(value),
                    )
                })
                .collect(),
        ),
    }
}
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn every_original_event_field_matches_both_modes_and_tick_page_variants() {
    let rows = rows();
    assert_eq!(rows.len(), 176);
    for row in rows {
        let cycle = row["cycle"].as_u64().unwrap() as usize;
        let ticks = row["ticks"].as_u64().unwrap();
        let page = row["page"].as_u64().unwrap();
        let original = unhex(row["wire"].as_str().unwrap());
        assert_eq!(
            checker::expected_event(cycle, ticks, page).unwrap(),
            row["event"],
            "{cycle}/{ticks}/{page}"
        );
        assert_eq!(
            checker::project_event(&original).unwrap(),
            row["event"],
            "full .to_dict schema semantics"
        );
        checker::compare_frame(cycle, ticks, page, &original).unwrap();
        let pss = row["event"]["procLog"]["procs"][0]["memPss"]
            .as_u64()
            .unwrap();
        assert_eq!(
            pss,
            if 2000 * page > 5 * 1024 * 1024 {
                if cycle < 20 {
                    50 * 1024
                } else {
                    99 * 1024
                }
            } else {
                0
            }
        );
        assert_eq!(row["event"]["procLog"]["procs"][1]["memPss"], 0);
    }
}
#[derive(Clone, Debug)]
enum Part {
    Key(String),
    Index(u32),
}
fn leaves(value: &Value, route: Vec<Part>, output: &mut Vec<Vec<Part>>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                let mut path = route.clone();
                path.push(Part::Key(key.clone()));
                leaves(value, path, output)
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                let mut path = route.clone();
                path.push(Part::Index(index as u32));
                leaves(value, path, output)
            }
        }
        _ => output.push(route),
    }
}
fn changed(value: Datum<'_>) -> Datum<'static> {
    match value {
        Datum::Bool(v) => Datum::Bool(!v),
        Datum::Int8(v) => Datum::Int8(v + 1),
        Datum::Int16(v) => Datum::Int16(v + 1),
        Datum::Int32(v) => Datum::Int32(v + 1),
        Datum::Int64(v) => Datum::Int64(v + 1),
        Datum::UInt8(v) => Datum::UInt8(v + 1),
        Datum::UInt16(v) => Datum::UInt16(v + 1),
        Datum::UInt32(v) => Datum::UInt32(v + 1),
        Datum::UInt64(v) => Datum::UInt64(v + 1),
        Datum::Float32(v) => Datum::Float32(v + 0.5),
        Datum::Float64(v) => Datum::Float64(v + 0.5),
        Datum::Text(_) => Datum::Text("changed original field".into()),
        _ => panic!("unexpected original leaf"),
    }
}
fn change(target: Dynamic<'_>, route: &[Part]) {
    match (target, &route[0]) {
        (Dynamic::Struct(mut target), Part::Key(name)) => {
            if route.len() == 1 {
                let value = changed(target.reborrow_as_reader().get_named(name).unwrap());
                target.set_named(name, value).unwrap();
            } else {
                change(target.get_named(name).unwrap(), &route[1..]);
            }
        }
        (Dynamic::List(mut target), Part::Index(index)) => {
            if route.len() == 1 {
                let value = changed(target.reborrow().into_reader().get(*index).unwrap());
                target.set(*index, value).unwrap();
            } else {
                change(target.get(*index).unwrap(), &route[1..]);
            }
        }
        _ => panic!("schema path differs: {route:?}"),
    }
}
#[test]
fn every_present_scalar_and_text_leaf_mutation_is_rejected() {
    let row = rows().remove(0);
    let bytes = unhex(row["wire"].as_str().unwrap());
    let source = capnp::serialize::read_message(bytes.as_slice(), Default::default()).unwrap();
    let mut paths = vec![];
    leaves(&row["event"], vec![], &mut paths);
    assert_eq!(paths.len(), 70);
    for path in paths {
        let mut message = capnp::message::Builder::new_default();
        message
            .set_root::<event::Owned>(source.get_root::<event::Reader<'_>>().unwrap())
            .unwrap();
        change(
            message.get_root::<event::Builder<'_>>().unwrap().into(),
            &path,
        );
        let altered = capnp::serialize::write_message_to_words(&message);
        assert!(
            checker::compare_frame(0, 100, 4096, &altered).is_err(),
            "silently ignored {path:?}"
        );
    }
    let invalid = include_bytes!("fixtures/proclog-reference/invalid-wire.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(invalid)),
        "d21939aee242535a34984a091dd32416b5d13091eb3145b4dade683239331cea"
    );
    for row in serde_json::from_slice::<Vec<Value>>(invalid).unwrap() {
        assert!(row["error"].is_string());
        assert!(checker::project_event(&unhex(row["hex"].as_str().unwrap())).is_err());
    }
}
#[test]
fn synthetic_procfs_bytes_and_fallback_are_original_including_invalid_utf8() {
    let raw = include_bytes!("fixtures/proclog-reference/procfs.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "c5bb52bbea86bc18cea63fe6d0095fde8dc6327d1276da4d69fc62c4386db64a"
    );
    let fixture: Value = serde_json::from_slice(raw).unwrap();
    for fallback in [false, true] {
        let root = tempfile::tempdir().unwrap();
        checker::prepare(root.path(), fallback).unwrap();
        let mode = if fallback { "smaps" } else { "smaps_rollup" };
        for (name, expected) in fixture[mode].as_object().unwrap() {
            let path = root.path().join(name);
            if let Some(target) = expected["symlink"].as_str() {
                assert_eq!(fs::read_link(path).unwrap(), Path::new(target));
            } else {
                assert_eq!(
                    fs::read(path).unwrap(),
                    unhex(expected["hex"].as_str().unwrap()),
                    "{name}"
                );
            }
        }
        checker::advance(root.path(), fallback, 1).unwrap();
        assert_eq!(
            fs::read(root.path().join("123").join(mode)).unwrap(),
            b"Pss: 99 kB\nPss_Anon: 80 kB\nPss_Shmem: 4 kB\n"
        );
    }
}
#[test]
fn queue_bytes_and_raw_cli_defaults_match_original() {
    let raw = include_bytes!("fixtures/proclog-reference/queue-int.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "c4e7b328453a4b7dd50b822d6ff9764fe84d62f5fba780befc1a49d50901a3d2"
    );
    let cases: Vec<Value> = serde_json::from_slice(raw).unwrap();
    assert_eq!(checker::queue_size(), 10485760);
    for case in cases {
        let parsed = checker::parse_queue_size(&unhex(case["hex"].as_str().unwrap()));
        assert_eq!(parsed.is_ok(), case["error"].is_null(), "{case:?}");
        if let Ok(value) = parsed {
            assert_eq!(value, case["value"].as_i64().unwrap());
        }
    }
    let root = Path::new("/private/source");
    assert_eq!(
        checker::arguments(&[], root).unwrap().unwrap(),
        root.join("rust/target/debug/examples/reference_trace")
    );
    let raw = OsString::from_vec(b"/private/candidate-\xff".to_vec());
    assert_eq!(
        checker::arguments(&["--binary".into(), raw.clone()], root)
            .unwrap()
            .unwrap()
            .as_os_str(),
        raw
    );
    for args in [
        vec!["--unknown".into()],
        vec!["--binary".into()],
        vec!["extra".into()],
    ] {
        assert!(checker::arguments(&args, root).is_err());
    }
    assert!(checker::arguments(&["--help".into()], root)
        .unwrap()
        .is_none());
}

#[cfg(target_os = "linux")]
#[path = "proclog_reference_check/process.rs"]
mod process;
