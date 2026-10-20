#![cfg(target_os = "linux")]
use openpilot_dev_tools::bluetooth_gesture_cases_generate as generator;
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn copied_root(destination: &Path) {
    for name in [
        "rust/tools/bluetooth_gesture_cases.py",
        "openpilot/selfdrive/carrot/bluetooth/tests/yiser_j6.json",
    ] {
        let target = destination.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        if root().join(name).is_file() {
            fs::copy(root().join(name), target).unwrap();
        } else {
            assert_eq!(name, "rust/tools/bluetooth_gesture_cases.py");
        }
    }
}
#[derive(Deserialize)]
struct RawEvent {
    at: Box<RawValue>,
}
#[derive(Deserialize)]
struct RawOperation {
    at: Option<Box<RawValue>>,
    event: Option<RawEvent>,
}
#[derive(Deserialize)]
struct RawCase {
    operations: Vec<RawOperation>,
}
fn raw_float(value: &RawValue) -> f64 {
    value.get().parse().unwrap()
}
#[test]
fn ordered_roster_boundary_float_bits_random_operations_and_recording_types() {
    let output = generator::generate(&root()).unwrap();
    let raw_rows = output
        .lines()
        .map(|line| serde_json::from_str::<RawCase>(line).unwrap())
        .collect::<Vec<_>>();
    let rows = output
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 37);
    for (index, row) in rows.iter().take(12).enumerate() {
        assert_eq!(
            row["profile"],
            if index < 6 { "generic" } else { "yiser-j6" }
        );
        assert_eq!(row["learning"], index % 2 == 1);
        assert_eq!(
            row["mapping"].as_object().unwrap().len(),
            if index % 6 < 2 { 0 } else { 27 }
        );
        assert_eq!(row["operations"], rows[0]["operations"]);
    }
    let shared = rows[0]["operations"].as_array().unwrap();
    let random = &shared[647..];
    assert_eq!(
        random.iter().filter(|op| op["op"] == "feed").count(),
        10_000
    );
    assert_eq!(
        random.iter().filter(|op| op["op"] == "flush").count(),
        10_000
    );
    assert!(random
        .iter()
        .all(|op| matches!(op["op"].as_str(), Some("feed" | "flush" | "cancel"))));
    for (index, boundary) in [0.0_f64, 0.35, 0.4, 0.7, 1.5, 10.0].iter().enumerate() {
        for (side, expected) in [boundary.next_down(), *boundary, boundary.next_up()]
            .iter()
            .enumerate()
        {
            let row = &rows[12 + index * 3 + side];
            assert_eq!(row["profile"], "generic");
            let lexical = raw_rows[12 + index * 3 + side].operations[2]
                .at
                .as_ref()
                .unwrap();
            let stamp = raw_float(lexical);
            assert_eq!(stamp.to_bits(), expected.to_bits());
            if side == 1 && (index == 0 || index == 5) {
                let integer = if index == 0 { "0" } else { "10" };
                assert_eq!(lexical.get(), integer);
                assert_eq!(
                    raw_rows[12 + index * 3 + side].operations[3]
                        .event
                        .as_ref()
                        .unwrap()
                        .at
                        .get(),
                    integer
                );
                assert_eq!(
                    raw_rows[12 + index * 3 + side].operations[4]
                        .event
                        .as_ref()
                        .unwrap()
                        .at
                        .get(),
                    integer
                );
            } else {
                assert!(lexical.get().contains('.') || lexical.get().contains('e'));
            }
        }
    }
    for (index, boundary) in [0.35_f64, 0.4].iter().enumerate() {
        for (side, expected) in [boundary.next_down(), *boundary, boundary.next_up()]
            .iter()
            .enumerate()
        {
            let stamp = raw_float(
                raw_rows[30 + index * 3 + side].operations[4]
                    .at
                    .as_ref()
                    .unwrap(),
            );
            assert_eq!(stamp.to_bits(), expected.to_bits());
        }
    }
    let recording: Vec<Vec<Box<RawValue>>> = serde_json::from_slice(
        &fs::read(root().join("openpilot/selfdrive/carrot/bluetooth/tests/yiser_j6.json")).unwrap(),
    )
    .unwrap();
    let actual = rows[36]["operations"].as_array().unwrap();
    assert_eq!(actual.len(), recording.len());
    for (index, (event, source)) in actual.iter().zip(&recording).enumerate() {
        assert_eq!(event["op"], "feed");
        let source_stamp = source[0].get();
        let output_stamp = raw_rows[36].operations[index]
            .event
            .as_ref()
            .unwrap()
            .at
            .get();
        assert_eq!(
            source_stamp.contains('.') || source_stamp.contains('e'),
            output_stamp.contains('.') || output_stamp.contains('e')
        );
        assert_eq!(
            output_stamp.parse::<f64>().unwrap().to_bits(),
            source_stamp.parse::<f64>().unwrap().to_bits()
        );
        assert_eq!(
            event["event"]["kind"].as_i64().unwrap(),
            source[1].get().parse::<i64>().unwrap()
        );
        assert_eq!(
            event["event"]["code"].as_i64().unwrap(),
            source[2].get().parse::<i64>().unwrap()
        );
        assert_eq!(
            event["event"]["value"].as_i64().unwrap(),
            source[3].get().parse::<i64>().unwrap()
        );
    }
}
#[test]
fn exclusive_output_and_source_pin_drift() {
    let temp = tempfile::tempdir().unwrap();
    let private = temp.path().join("source");
    copied_root(&private);
    let output = temp.path().join("input.jsonl");
    generator::write(&private, &output).unwrap();
    let original = fs::read(&output).unwrap();
    assert!(generator::write(&private, &output).is_err());
    assert_eq!(fs::read(&output).unwrap(), original);
    fs::write(
        private.join("rust/tools/bluetooth_gesture_cases.py"),
        b"changed helper",
    )
    .unwrap();
    let other = temp.path().join("other.jsonl");
    assert!(generator::write(&private, &other).is_err());
    assert!(!other.exists());
}

#[test]
fn exact_historical_helper_absence_keeps_corpus_and_other_sources_strict() {
    let temp = tempfile::tempdir().unwrap();
    let private = temp.path().join("source");
    copied_root(&private);
    let expected = generator::generate(&private).unwrap();
    let helper = private.join("rust/tools/bluetooth_gesture_cases.py");
    if helper.exists() {
        fs::remove_file(&helper).unwrap();
    }
    let absent = generator::generate(&private).unwrap();
    assert_eq!(absent.as_bytes(), expected.as_bytes());
    let output = temp.path().join("absent.jsonl");
    generator::write(&private, &output).unwrap();
    assert_eq!(fs::read(&output).unwrap(), expected.as_bytes());
    fs::write(&helper, b"unexpected reappearance").unwrap();
    assert!(generator::generate(&private).is_err());
    fs::remove_file(&helper).unwrap();
    std::os::unix::fs::symlink(
        root().join("rust/tools/bluetooth_gesture_cases.py"),
        &helper,
    )
    .unwrap();
    assert!(generator::generate(&private).is_err());
    fs::remove_file(&helper).unwrap();
    fs::remove_file(private.join("openpilot/selfdrive/carrot/bluetooth/tests/yiser_j6.json"))
        .unwrap();
    assert!(generator::generate(&private).is_err());
    fs::remove_dir(private.join("rust/tools")).unwrap();
    assert!(generator::generate(&private).is_err());
}
