#![cfg(target_os = "linux")]

use openpilot_dev_tools::updated_process_check;
#[test]
fn fixture_identity_binds_the_running_binary_to_the_recorded_source() {
    let output = Command::new(env!("CARGO_BIN_EXE_updated-process-fixture"))
        .arg("identity")
        .output()
        .unwrap();
    assert!(output.status.success());
    let identity: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(identity["schema"], "updated-process-fixture-v1");
    assert_eq!(
        identity["source_sha256"],
        openpilot_dev_tools::updated_process_fixture::source_sha256()
    );
    assert_eq!(identity["source_sha256"].as_str().unwrap().len(), 64);
}
use std::{fs, process::Command};
#[test]
fn fixture_preserves_separate_raw_streams_and_nonzero_status() {
    let output = Command::new(env!("CARGO_BIN_EXE_updated-process-fixture"))
        .arg("merged-output")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"one\r\n");
    assert_eq!(output.stderr, b"two\r");
    let output = Command::new(env!("CARGO_BIN_EXE_updated-process-fixture"))
        .arg("nonzero")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"failure detail\n");
}
#[test]
fn missing_or_modified_baseline_fails_before_any_child_is_started() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing.json");
    assert!(updated_process_check::load_baseline(&missing).is_err());
    let modified = temp.path().join("modified.json");
    fs::write(&modified, b"{\"schema_version\":1}").unwrap();
    assert!(updated_process_check::load_baseline(&modified).is_err());
    let output = temp.path().join("output");
    let result = Command::new(env!("CARGO_BIN_EXE_check-updated-process"))
        .args([
            "--binary",
            "/must-not-execute-candidate",
            "--launcher",
            "/must-not-execute-launcher",
            "--fixture",
            "/must-not-execute-fixture",
            "--baseline",
        ])
        .arg(&modified)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    assert!(result.stdout.is_empty());
}
#[test]
fn unknown_duplicate_and_missing_cli_options_fail_before_output_creation() {
    for args in [
        vec!["--unknown"],
        vec!["--binary", "first", "--binary", "second"],
        vec!["--output"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_check-updated-process"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
    }
}
#[test]
fn original_process_probe_error_and_state_cases_reach_the_checker_consumer() {
    use std::io;
    for errno in [2, 3] {
        assert!(
            !updated_process_check::alive_from(|| Err(io::Error::from_raw_os_error(errno)))
                .unwrap()
        );
    }
    for (state, expected) in [('Z', false), ('S', true)] {
        assert_eq!(
            updated_process_check::alive_from(|| Ok(format!("123 (worker) {state} 1 2 3")))
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        updated_process_check::alive_from(|| Err(io::Error::from_raw_os_error(13)))
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
}
#[test]
fn checked_in_original_baseline_keeps_source_identity_and_all_five_cases() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/updated-process/original.json");
    let baseline = updated_process_check::load_baseline(&path).unwrap();
    assert_eq!(
        baseline["source_commit"],
        "b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a"
    );
    assert_eq!(baseline["normal"].as_array().unwrap().len(), 3);
    assert_eq!(baseline["groups"].as_array().unwrap().len(), 2);
    assert_eq!(
        baseline["normal"][0]["result"],
        serde_json::json!({"output":"one\ntwo\n","code":0})
    );
    assert_eq!(
        baseline["normal"][1]["result"],
        serde_json::json!({"output":"failure detail\n","code":7})
    );
    assert_eq!(
        baseline["normal"][2]["result"],
        serde_json::json!({"output":"keep\n0\nfalse\nfalse\n","code":0})
    );
}
