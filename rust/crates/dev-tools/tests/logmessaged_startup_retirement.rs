//! Cloud-only current/absent proof against the sealed original startup observations.
#![cfg(all(target_os = "linux", feature = "logmessaged-startup-native"))]

use openpilot_cereal::log_capnp::event;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

const CASES: [&str; 3] = ["legacy-source", "ready-source", "ready-native"];
const HISTORICAL: &str = "rust/tools/check_logmessaged_startup.py";
const ORIGINAL_MARKER: &str = r#"{"msg": "first-error-after-start", "levelnum": 40}"#;
const ORIGINAL_MANIFEST_SHA256: &str =
    "cee8e19032542bc4851d0ce6bfebd9b71dde9184b19bdc5b0fb23e06e974d0dd";
const ORIGINAL_REPORT_SHA256: &str =
    "ee463cc49825ed060521c243600313f99a1f3fd88353c89329da8c846718075e";

fn required(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).expect(name));
    assert!(path.is_absolute(), "{name} must be absolute");
    path
}
fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn hash(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn original_fixture() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/logmessaged-startup/13d-original");
    let manifest = root.join("SHA256SUMS");
    assert_eq!(hash(&manifest), ORIGINAL_MANIFEST_SHA256);
    let records = fs::read_to_string(manifest).unwrap();
    assert_eq!(records.lines().count(), 18);
    for row in records.lines() {
        let (expected, name) = row.split_once("  ").unwrap();
        assert_eq!(hash(&root.join(name)), expected, "sealed original {name}");
    }
    assert_eq!(hash(&root.join("report.json")), ORIGINAL_REPORT_SHA256);
    root
}
fn check_sources(root: &Path) {
    let pins: std::collections::BTreeMap<String, String> = serde_json::from_str(include_str!(
        "fixtures/logmessaged-startup/source-pins.json"
    ))
    .unwrap();
    for (name, expected) in pins {
        let path = root.join(&name);
        if name == HISTORICAL {
            assert!(fs::symlink_metadata(path.parent().unwrap())
                .unwrap()
                .file_type()
                .is_dir());
            assert_eq!(
                fs::symlink_metadata(&path).unwrap_err().kind(),
                std::io::ErrorKind::NotFound
            );
            continue;
        }
        assert!(
            fs::symlink_metadata(&path).unwrap().file_type().is_file(),
            "{name}"
        );
        assert_eq!(hash(&path), expected, "{name}");
    }
}
fn packets(path: &Path, service: &str) -> Vec<String> {
    let bytes = match fs::read(path.join(format!("{service}.bin"))) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return vec![],
        Err(error) => panic!("{error}"),
    };
    let mut input = Cursor::new(bytes);
    let mut rows = vec![];
    while input.position() < input.get_ref().len() as u64 {
        let message = capnp::serialize::read_message(&mut input, Default::default()).unwrap();
        let event = message.get_root::<event::Reader<'_>>().unwrap();
        assert!(event.get_valid());
        let text = match (service, event.which().unwrap()) {
            ("logMessage", event::LogMessage(value))
            | ("errorLogMessage", event::ErrorLogMessage(value)) => value.unwrap(),
            _ => panic!("unexpected startup event union"),
        };
        rows.push(text.to_str().unwrap().to_owned());
    }
    rows
}
fn disk(path: &Path) -> Vec<Value> {
    let home = path.join("collector/home");
    let mut rows = vec![];
    for directory in fs::read_dir(home).unwrap() {
        let directory = directory.unwrap();
        if !directory
            .file_name()
            .as_encoded_bytes()
            .starts_with(b".comma")
        {
            continue;
        }
        for file in fs::read_dir(directory.path().join("log")).unwrap() {
            let file = file.unwrap();
            if file.file_name().as_encoded_bytes().starts_with(b"swaglog.") {
                rows.extend(
                    fs::read_to_string(file.path())
                        .unwrap()
                        .lines()
                        .map(|line| {
                            let mut value: Value = serde_json::from_str(line).unwrap();
                            let id = value.as_object_mut().unwrap().remove("id").unwrap();
                            let id = id.as_str().unwrap();
                            let plain = id.replace('-', "");
                            assert_eq!(plain.len(), 32);
                            assert!(plain.bytes().all(|v| v.is_ascii_hexdigit()));
                            if id.len() != 32 {
                                assert_eq!(id.len(), 36);
                                for at in [8, 13, 18, 23] {
                                    assert_eq!(id.as_bytes()[at], b'-');
                                }
                            }
                            assert_eq!(plain.as_bytes()[12].to_ascii_lowercase(), b'4');
                            assert!(matches!(
                                plain.as_bytes()[16].to_ascii_lowercase(),
                                b'8' | b'9' | b'a' | b'b'
                            ));
                            value
                        }),
                );
            }
        }
    }
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0],
        json!({"msg$s":"first-error-after-start","levelnum":40})
    );
    rows
}
fn execs(trace: &Path, checker: &Path, collector: &Path, output: &Path) {
    let text = fs::read_to_string(trace).unwrap();
    assert!(!text.contains("execveat("), "unaccounted execveat attempt");
    assert_eq!(
        text.matches("execve(").count(),
        7,
        "extra or missing execve attempt"
    );
    let expression =
        regex::Regex::new(r#"execve\(("(?:[^"\\]|\\.)*"), (\[.*?\]), .*\) = 0"#).unwrap();
    let calls: Vec<(String, Vec<String>)> = expression
        .captures_iter(&text)
        .map(|match_| {
            (
                serde_json::from_str(&match_[1]).unwrap(),
                serde_json::from_str(&match_[2]).unwrap(),
            )
        })
        .collect();
    assert_eq!(calls.len(), 7, "failed or unparsable exec attempt");
    let checker = checker.to_str().unwrap();
    let collector = collector.to_str().unwrap();
    assert_eq!(
        calls[0],
        (
            checker.into(),
            vec![
                checker.into(),
                "--collector".into(),
                collector.into(),
                "--output".into(),
                output.to_str().unwrap().into()
            ]
        )
    );
    for (index, case) in CASES.iter().enumerate() {
        let worker = &calls[1 + 2 * index];
        assert_eq!(worker.0, checker);
        assert_eq!(
            &worker.1[..4],
            &[checker, "--startup-worker", *case, collector]
        );
        assert_eq!(worker.1.len(), 6);
        assert_eq!(worker.1[4], output.join(*case).to_str().unwrap());
        assert!(worker.1[5].starts_with("/tmp/log-startup-"));
        let child = &calls[2 + 2 * index];
        if *case == "ready-native" {
            assert_eq!(child, &(collector.into(), vec![collector.into()]));
        } else {
            assert_eq!(child.0, checker);
            assert_eq!(child.1.len(), 5);
            assert_eq!(&child.1[..2], &[checker, "--startup-source"]);
            assert!(child.1[2].starts_with("ipc:///tmp/logmessage"));
            assert!(
                child.1[3].starts_with(output.join(*case).join("collector/home").to_str().unwrap())
            );
            assert_eq!(child.1[4], format!("{}/publisher", worker.1[5]));
        }
    }
}

#[test]
fn same_elf_current_and_absent_lanes_match_sealed_original_startup() {
    let proof = required("LOGMESSAGED_STARTUP_RETIREMENT_PROOF");
    let original = original_fixture();
    let checker = required("LOGMESSAGED_STARTUP_RETIREMENT_CHECKER");
    let collector = required("LOGMESSAGED_STARTUP_RETIREMENT_COLLECTOR");
    let current_root = required("LOGMESSAGED_STARTUP_RETIREMENT_CURRENT_ROOT");
    let absent_root = required("LOGMESSAGED_STARTUP_RETIREMENT_ABSENT_ROOT");
    assert_ne!(current_root, absent_root);
    assert_eq!(
        fs::canonicalize(&current_root).unwrap(),
        fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")).unwrap()
    );
    check_sources(&current_root);
    check_sources(&absent_root);
    let original_report = read_json(&original.join("report.json"));
    assert_eq!(original_report["passed"], true);
    assert_eq!(
        original_report["cases"],
        json!({"legacy-source":{"logMessage":1,"errorLogMessage":0},"ready-source":{"logMessage":1,"errorLogMessage":1},"ready-native":{"logMessage":1,"errorLogMessage":1}})
    );
    let present = fs::read(proof.join("current/report.json")).unwrap();
    let absent = fs::read(proof.join("absent/report.json")).unwrap();
    assert_eq!(
        present, absent,
        "the copied absent root must not affect the native report"
    );
    let report: Value = serde_json::from_slice(&present).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["cases"], original_report["cases"]);
    assert_eq!(report["fixture_sha256"], original_report["fixture_sha256"]);
    assert_eq!(
        report["historical_source_sha256"][HISTORICAL],
        "46cae42fe4749a48bb9f0acc81b92db974288633d28100b0a023b3d002b9b022"
    );
    assert!(report["input_sha256"].get(HISTORICAL).is_none());
    assert_eq!(report["input_sha256"]["checker"], hash(&checker));
    assert_eq!(report["input_sha256"]["collector"], hash(&collector));
    for lane in ["current", "absent"] {
        let output = proof.join(lane);
        assert_eq!(
            fs::read(output.with_extension("stdout")).unwrap(),
            [
                fs::read(output.join("report.json")).unwrap(),
                b"\n".to_vec()
            ]
            .concat()
        );
        execs(
            &output.with_extension("exec"),
            &checker,
            &collector,
            &output,
        );
        for case in CASES {
            let expected: Value = read_json(&original.join(case).join("observed.json"));
            assert_eq!(
                expected,
                json!({"logMessage":[ORIGINAL_MARKER], "errorLogMessage":
                    if case == "legacy-source" { json!([]) } else { json!([ORIGINAL_MARKER]) }})
            );
            assert_eq!(
                read_json(&output.join(case).join("observed.json")),
                expected
            );
            for service in ["logMessage", "errorLogMessage"] {
                let expected_packets: Vec<String> =
                    serde_json::from_value(expected[service].clone()).unwrap();
                assert_eq!(packets(&original.join(case), service), expected_packets);
                assert_eq!(packets(&output.join(case), service), expected_packets);
            }
            assert_eq!(disk(&output.join(case)), disk(&original.join(case)));
            let exit = read_json(&output.join(case).join("collector/exit.json"));
            let original_exit = read_json(&original.join(case).join("collector/exit.json"));
            assert_eq!(exit["signal"], 2);
            assert_eq!(exit["code"], if case == "ready-native" { 0 } else { -2 });
            assert_eq!(exit["code"], original_exit["code"]);
            assert_eq!(exit["signal"], original_exit["signal"]);
        }
    }
}
