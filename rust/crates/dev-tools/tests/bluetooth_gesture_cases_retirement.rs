//! Cloud-only same-ELF proof against the actual original-only 9845 observation.
#![cfg(target_os = "linux")]
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const HELPER: &str = "rust/tools/bluetooth_gesture_cases.py";
const RECORDING: &str = "openpilot/selfdrive/carrot/bluetooth/tests/yiser_j6.json";

fn required(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")));
    assert!(path.is_absolute(), "{name} must be absolute");
    path
}
fn sha(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn one_native_exec(trace: &Path, binary: &Path, root: &Path, output: &Path) {
    let text = fs::read_to_string(trace).unwrap();
    assert_eq!(text.matches("execve(").count(), 1, "extra child execution");
    assert!(!text.contains("execveat("), "opaque child execution");
    let expression =
        regex::Regex::new(r#"execve\(("(?:[^"\\]|\\.)*"), (\[.*?\]), .*\) = 0"#).unwrap();
    let capture = expression
        .captures(&text)
        .expect("successful native exec missing");
    let program: String = serde_json::from_str(&capture[1]).unwrap();
    let argv: Vec<String> = serde_json::from_str(&capture[2]).unwrap();
    let path = binary.to_str().unwrap();
    assert_eq!(program, path);
    assert_eq!(
        argv,
        vec![
            path.to_owned(),
            "--root".into(),
            root.to_str().unwrap().into(),
            "--output".into(),
            output.to_str().unwrap().into(),
        ]
    );
}

#[test]
fn original_observation_present_and_absent_same_elf() {
    let observation: Value = serde_json::from_slice(include_bytes!(
        "fixtures/bluetooth-gesture-cases/original-observation-9845.json"
    ))
    .unwrap();
    let historical = include_bytes!("fixtures/bluetooth-gesture-cases/original-result-9845.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(historical)),
        observation["original_result_sha256"]
    );
    assert_eq!(
        serde_json::from_slice::<Value>(historical).unwrap(),
        observation["original_result"]
    );
    assert_eq!(observation["candidate_executed_for_expectations"], false);
    let original = required("BLUETOOTH_GESTURE_ORIGINAL_OUTPUT");
    let absent_check = required("BLUETOOTH_GESTURE_ABSENT_CHECK_OUTPUT");
    let present_root = required("BLUETOOTH_GESTURE_PRESENT_ROOT");
    let absent_root = required("BLUETOOTH_GESTURE_ABSENT_ROOT");
    let present = required("BLUETOOTH_GESTURE_PRESENT_INPUT");
    let absent = required("BLUETOOTH_GESTURE_ABSENT_INPUT");
    let binary = required("BLUETOOTH_GESTURE_GENERATOR_BINARY");
    let candidate = required("BLUETOOTH_GESTURE_CANDIDATE_BINARY");
    assert!(present_root.join(HELPER).is_file());
    assert_eq!(
        sha(&present_root.join(HELPER)),
        observation["original_generator_sha256"]
    );
    assert_eq!(
        sha(&present_root.join(RECORDING)),
        observation["recording_sha256"]
    );
    assert!(!absent_root.join(HELPER).exists());
    assert_eq!(
        fs::symlink_metadata(absent_root.join(HELPER))
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(
        sha(&absent_root.join(RECORDING)),
        observation["recording_sha256"]
    );
    assert_eq!(fs::read(&present).unwrap(), fs::read(&absent).unwrap());
    assert_eq!(sha(&present), observation["original_input_sha256"]);
    assert_eq!(
        sha(&original.join("input.jsonl")),
        observation["original_input_sha256"]
    );
    assert_eq!(
        fs::read(&present).unwrap(),
        fs::read(original.join("input.jsonl")).unwrap()
    );
    assert_eq!(
        sha(&original.join("original.jsonl")),
        observation["original_decoder_output_sha256"]
    );
    let report = read(&original.join("result.json"));
    assert_eq!(report["cases"], 37);
    assert_eq!(report["steps"], 260012);
    assert_eq!(report["tokens"], 1024);
    assert_eq!(report["source_sha256"], observation["model_source_sha256"]);
    assert_eq!(report["binary_sha256"], sha(&candidate));
    assert_eq!(read(&absent_check.join("result.json")), report);
    for name in [
        "input.jsonl",
        "original.jsonl",
        "native.jsonl",
        "native.stderr",
    ] {
        assert_eq!(
            fs::read(original.join(name)).unwrap(),
            fs::read(absent_check.join(name)).unwrap(),
            "{name}"
        );
    }
    assert_eq!(observation["original_result"]["cases"], report["cases"]);
    assert_eq!(observation["original_result"]["steps"], report["steps"]);
    assert_eq!(observation["original_result"]["tokens"], report["tokens"]);
    assert_eq!(
        observation["original_result"]["source_sha256"],
        report["source_sha256"]
    );
    for (lane, root, output) in [
        ("PRESENT", &present_root, &present),
        ("ABSENT", &absent_root, &absent),
    ] {
        let trace = required(&format!("BLUETOOTH_GESTURE_{lane}_EXEC"));
        one_native_exec(&trace, &binary, root, output);
    }
    assert_eq!(
        fs::read(&present)
            .unwrap()
            .iter()
            .filter(|&&byte| byte == b'\n')
            .count(),
        37
    );
}
