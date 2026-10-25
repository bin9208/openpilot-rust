//! Independent unchanged-original expectations, captured before native code.
use openpilot_dev_tools::lagd_reference::{numeric, sources};
use openpilot_log_format::JsonValue;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

const MANIFEST_SHA: &str = "26ef2f7cb7e295ff4315e121f9cfd63c60100aa51e4c0727f54623fb3bef7ab0";

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lagd-tools")
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn compressed(name: &str) -> Value {
    let bytes = fs::read(fixture_root().join(name)).unwrap();
    let mut plain = Vec::new();
    flate2::read::GzDecoder::new(bytes.as_slice())
        .read_to_end(&mut plain)
        .unwrap();
    serde_json::from_slice(&plain).unwrap()
}

fn original_number(value: &Value) -> Option<f64> {
    // The original comparison admits bool as a Python int subclass.
    value
        .as_f64()
        .or_else(|| value.as_bool().map(|v| if v { 1.0 } else { 0.0 }))
}

fn compare_original(expected: &Value, actual: &Value, budget: &Value, path: &str) {
    match expected {
        Value::Object(want) => {
            let got = actual.as_object().expect(path);
            assert_eq!(
                want.keys().collect::<Vec<_>>(),
                got.keys().collect::<Vec<_>>(),
                "{path}"
            );
            for (key, value) in want {
                compare_original(value, &got[key], budget, &format!("{path}.{key}"));
            }
        }
        Value::Array(want) => {
            let got = actual.as_array().expect(path);
            assert_eq!(want.len(), got.len(), "{path}");
            for (index, (want, got)) in want.iter().zip(got).enumerate() {
                compare_original(want, got, budget, &format!("{path}[{index}]"));
            }
        }
        Value::Number(number) if number.is_f64() => {
            let want = number.as_f64().unwrap();
            let got = original_number(actual).expect(path);
            assert!(want.is_finite() && got.is_finite(), "{path}");
            let absolute = budget["absolute"].as_f64().unwrap();
            let relative = budget["relative"].as_f64().unwrap();
            let tolerance = absolute
                .max(relative * want.abs())
                .max(relative * got.abs());
            assert!(
                (want - got).abs() <= tolerance,
                "{path}: source={want:?}, native={got:?}, budget={budget}"
            );
        }
        Value::Number(_) | Value::Bool(_) => {
            assert_eq!(original_number(expected), original_number(actual), "{path}");
        }
        _ => assert_eq!(expected, actual, "{path}"),
    }
}

#[test]
fn immutable_original_corpus_covers_all_fixed_cases_and_full_loop_snapshots() {
    let raw = fs::read(fixture_root().join("manifest.json")).unwrap();
    assert_eq!(sha(&raw), MANIFEST_SHA);
    let manifest: Value = serde_json::from_slice(&raw).unwrap();
    for (name, expected) in manifest["files"].as_object().unwrap() {
        let bytes = fs::read(fixture_root().join(name)).unwrap();
        assert_eq!(sha(&bytes), expected["sha256"], "{name}");
        assert_eq!(
            bytes.len() as u64,
            expected["bytes"].as_u64().unwrap(),
            "{name}"
        );
    }
    assert_eq!(
        compressed("numeric-original.json.gz")
            .as_array()
            .unwrap()
            .len(),
        95
    );
    assert_eq!(
        compressed("cache-original.json.gz")
            .as_array()
            .unwrap()
            .len(),
        14
    );
    assert_eq!(
        compressed("packet-original.json.gz")
            .as_array()
            .unwrap()
            .len(),
        13
    );
    let mut total = 0;
    for (mode, count, publications) in [
        ("normal", 1201, 241),
        ("recovery", 1301, 261),
        ("ordering", 1201, 241),
    ] {
        let value = compressed(&format!("loop-{mode}-original.json.gz"));
        let frames = value["frames"].as_array().unwrap();
        let source = value["source"].as_array().unwrap();
        assert_eq!(frames.len(), count);
        assert_eq!(source.len(), count);
        assert_eq!(
            source.iter().filter(|row| row["publish"] == true).count(),
            publications
        );
        assert_eq!(
            source
                .iter()
                .filter(|row| row["persist"] == true)
                .map(|row| row["frame"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            [0, 1200]
        );
        for row in source {
            assert!(
                row["state"]["recovery"].is_array(),
                "all nonpublication states are retained too"
            );
        }
        total += count;
    }
    assert_eq!(total, 3703);
}

#[test]
fn all_95_original_numeric_outputs_match_fixed_budgets_and_exact_discrete_values() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let raw = fs::read(root.join("rust/crates/lagd/tests/tolerances.json")).unwrap();
    assert_eq!(
        sha(&raw),
        "5f8d76478bf89581b2c7e25e7e122cd61fb3ddcb3cf63f59ece3b2c9c22e766b"
    );
    let budgets: Value = serde_json::from_slice(&raw).unwrap();
    let rows = compressed("numeric-original.json.gz");
    for row in rows.as_array().unwrap() {
        let input = JsonValue::parse(&serde_json::to_string(&row["input"]).unwrap()).unwrap();
        let output = numeric(&input).unwrap();
        let output: Value = serde_json::from_str(&output.to_json().unwrap()).unwrap();
        compare_original(
            &row["source"],
            &output,
            &budgets[row["budget"].as_str().unwrap()],
            row["case"].as_str().unwrap(),
        );
    }
}

#[test]
fn each_source_pin_checks_present_bytes_and_all_live_inputs_remain_required() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    sources(&root).unwrap();
    let pins: Value =
        serde_json::from_slice(&fs::read(fixture_root().join("source-pins.json")).unwrap())
            .unwrap();
    let pins = pins["sources"].as_object().unwrap();
    assert_eq!(pins.len(), 25);
    let copy = tempfile::tempdir().unwrap();
    for name in pins.keys() {
        let destination = copy.path().join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        match fs::copy(root.join(name), destination) {
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                assert!(openpilot_dev_tools::lagd_reference::retired_source(name));
            }
            Err(error) => panic!("{name}: {error}"),
        }
    }
    sources(copy.path()).unwrap();
    for name in pins.keys() {
        let path = copy.path().join(name);
        let bytes = fs::read(&path).ok();
        if path.exists() {
            fs::remove_file(&path).unwrap();
        }
        assert_eq!(
            sources(copy.path()).is_ok(),
            openpilot_dev_tools::lagd_reference::retired_source(name),
            "missing source boundary: {name}"
        );
        let mut changed = bytes.clone().unwrap_or_default();
        changed.push(b'\n');
        fs::write(&path, changed).unwrap();
        assert!(
            sources(copy.path()).is_err(),
            "changed source accepted: {name}"
        );
        if let Some(bytes) = bytes {
            fs::write(path, bytes).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }
}
