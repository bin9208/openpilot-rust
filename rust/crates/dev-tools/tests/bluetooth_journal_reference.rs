#![cfg(feature = "bluetooth-journal-native")]
//! Independent unchanged-original observation against fresh, same-ELF lanes.
use openpilot_dev_tools::bluetooth_journal_check::{self as journal, OPERATIONS};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("required journal evidence missing: {name}"))
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn rows(path: &Path) -> Vec<String> {
    let trace = journal::parse_trace(&fs::read_to_string(path).unwrap()).unwrap();
    let normalized = journal::normalize_trace(&trace).unwrap();
    assert_eq!(normalized.rows.len(), OPERATIONS);
    assert_eq!(normalized.sessions.len(), 1);
    assert_eq!(normalized.max_events, 64);
    normalized
        .rows
        .iter()
        .map(|row| hash(row.to_json().unwrap().as_bytes()))
        .collect()
}
fn report(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path.join("result.json")).unwrap()).unwrap()
}
fn audit_exec(trace: &Path, checker: &Path, fixture: &Path, output: &Path) {
    let text = fs::read_to_string(trace).unwrap();
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let invocation =
        regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let lines = text
        .lines()
        .filter(|line| opening.is_match(line))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "unexpected journal executable attempt");
    let mut actual = BTreeMap::new();
    for line in lines {
        let capture = invocation
            .captures(line)
            .expect("opaque or failed journal exec");
        let executable: String = serde_json::from_str(&capture[1]).unwrap();
        let argv: Vec<String> = serde_json::from_str(&capture[2]).unwrap();
        *actual.entry((executable, argv)).or_insert(0usize) += 1;
    }
    let checker = checker.to_str().unwrap().to_owned();
    let fixture = fixture.to_str().unwrap().to_owned();
    let output = output.to_str().unwrap().to_owned();
    assert_eq!(
        actual,
        BTreeMap::from([
            (
                (
                    checker.clone(),
                    vec![
                        checker,
                        "--binary".into(),
                        fixture.clone(),
                        "--output".into(),
                        output.clone()
                    ]
                ),
                1
            ),
            (
                (fixture.clone(), vec![fixture, format!("{output}/native")]),
                1
            ),
        ])
    );
}

#[test]
fn sealed_original_and_fresh_present_absent_lanes_match_all_721_decisions() {
    let sealed: Value = serde_json::from_str(include_str!(
        "fixtures/bluetooth-journal/original-observation-39507901490.json"
    ))
    .unwrap();
    let original = required("BLUETOOTH_JOURNAL_ORIGINAL_OUTPUT");
    let present = required("BLUETOOTH_JOURNAL_PRESENT_OUTPUT");
    let absent = required("BLUETOOTH_JOURNAL_ABSENT_OUTPUT");
    let historical =
        required("BLUETOOTH_JOURNAL_PRESENT_ROOT").join("rust/tools/check_bluetooth_journal.py");
    assert_eq!(
        hash(&fs::read(historical).unwrap()),
        sealed["observation"]["source_checker_sha256"]
            .as_str()
            .unwrap(),
        "present lane must retain the unchanged historical checker"
    );
    // Fresh original UUIDs and inode numbers differ from the sealed run;
    // compare every normalized source row with its independent digest below.
    assert_eq!(
        hash(&fs::read(original.join("input.jsonl")).unwrap()),
        sealed["observation"]["input_sha256"].as_str().unwrap()
    );
    let expected = sealed["row_sha256"].as_array().unwrap();
    assert_eq!(expected.len(), OPERATIONS);
    for (name, path) in [
        ("original", &original),
        ("present", &present),
        ("absent", &absent),
    ] {
        let observed = rows(&path.join(if name == "original" {
            "original.jsonl"
        } else {
            "native.jsonl"
        }));
        let observed = observed.into_iter().map(Value::String).collect::<Vec<_>>();
        assert_eq!(
            &observed, expected,
            "{name}: normalized original operation/rewrite decision"
        );
        let input = fs::read(path.join("input.jsonl")).unwrap();
        assert_eq!(
            hash(&input),
            sealed["observation"]["input_sha256"].as_str().unwrap(),
            "{name}: input schedule"
        );
    }
    let original_report = report(&original);
    let present_report = report(&present);
    let absent_report = report(&absent);
    for key in [
        "operations",
        "max_events_per_channel",
        "mode",
        "values_and_rewrite_decisions",
        "source_sha256",
        "binary_sha256",
    ] {
        assert_eq!(present_report[key], original_report[key], "present {key}");
        assert_eq!(absent_report[key], original_report[key], "absent {key}");
    }
    assert_eq!(present_report["operations"], OPERATIONS);
    assert_eq!(present_report["max_events_per_channel"], 64);
    assert_eq!(
        original_report["source_sha256"],
        sealed["observation"]["model_sha256"]
    );
    for path in [&present, &absent] {
        let mut names = fs::read_dir(path.join("native"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            [OsString::from("cruise.json"), OsString::from("lane.json")],
            "unremoved native journal temporary file"
        );
    }
    let checker = required("BLUETOOTH_JOURNAL_CHECKER");
    let fixture = required("BLUETOOTH_JOURNAL_FIXTURE");
    audit_exec(
        &required("BLUETOOTH_JOURNAL_PRESENT_EXEC"),
        &checker,
        &fixture,
        &present,
    );
    audit_exec(
        &required("BLUETOOTH_JOURNAL_ABSENT_EXEC"),
        &checker,
        &fixture,
        &absent,
    );
    let source =
        required("BLUETOOTH_JOURNAL_ABSENCE_ROOT").join("rust/tools/check_bluetooth_journal.py");
    assert!(
        matches!(fs::symlink_metadata(source), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
    );
    let before: BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(required("BLUETOOTH_JOURNAL_ARTIFACTS_BEFORE")).unwrap())
            .unwrap();
    let after: BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(required("BLUETOOTH_JOURNAL_ARTIFACTS_AFTER")).unwrap())
            .unwrap();
    assert_eq!(before, after, "checker or fixture changed between lanes");
    assert_eq!(before.len(), 2);
    for binary in [&checker, &fixture] {
        assert_eq!(
            before[binary.to_str().unwrap()],
            hash(&fs::read(binary).unwrap())
        );
    }
}
