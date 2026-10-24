//! Actual checker child owner and full real-IPC comparison; no optional early return.
use openpilot_dev_tools::feedbackd_check::validate_source;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn checker() -> &'static str {
    env!("CARGO_BIN_EXE_check-feedbackd")
}
#[test]
fn actual_current_source_native_daemon_retains_complete_original_eight_iteration_checks() {
    validate_source(&root()).unwrap();
    let binary = std::env::var_os("FEEDBACKD_NATIVE_BINARY")
        .expect("mandatory current-source feedbackd executable");
    let tmp = tempfile::tempdir().unwrap();
    let output = tmp.path().join("actual");
    let observed = Command::new(checker())
        .args(["--binary"])
        .arg(binary)
        .arg("--output")
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", root())
        .output()
        .unwrap();
    assert!(
        observed.status.success(),
        "{}",
        String::from_utf8_lossy(&observed.stderr)
    );
    assert_eq!(
        observed.stdout,
        b"PASS source/native feedback IPC, validity, logging, disabled recording and shutdown\n"
    );
    let report: Value =
        serde_json::from_slice(&fs::read(output.join("comparison.json")).unwrap()).unwrap();
    assert_eq!(report["source_native_equal"], true);
    assert_eq!(report["audio_recording_disabled"], true);
    assert_eq!(report["rows"].as_array().unwrap().len(), 8);
    for kind in ["source", "native"] {
        let folder = output.join(kind);
        for i in [0, 2, 4, 6] {
            assert!(folder.join(format!("bookmark-{i}.capnp")).is_file());
        }
        let lifecycle: Value =
            serde_json::from_slice(&fs::read(folder.join("lifecycle.json")).unwrap()).unwrap();
        assert_eq!(lifecycle["exit_code"], 0);
        assert_eq!(lifecycle["child_reaped"], true);
        assert_eq!(lifecycle["namespace_removed"], true);
        assert!(!Path::new(lifecycle["namespace"].as_str().unwrap()).exists());
    }
}
#[test]
fn actual_early_child_exit_keeps_logs_and_cleanup_and_is_not_hidden_as_timeout() {
    validate_source(&root()).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let output = tmp.path().join("failed");
    let observed = Command::new(checker())
        .args(["--binary", "/bin/false", "--output"])
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", root())
        .output()
        .unwrap();
    assert_eq!(observed.status.code(), Some(1));
    assert!(output.join("native/stdout.log").is_file());
    assert!(output.join("native/stderr.log").is_file());
    assert!(!output.join("comparison.json").exists());
    let failure: Value =
        serde_json::from_slice(&fs::read(output.join("native/lifecycle.json")).unwrap()).unwrap();
    assert_eq!(failure["phase"], "readiness");
    assert_eq!(failure["exit_code"], 1);
    assert_eq!(failure["timed_out"], false);
    assert_eq!(failure["child_reaped"], true);
    assert_eq!(failure["namespace_removed"], true);
    assert!(!Path::new(failure["namespace"].as_str().unwrap()).exists());
}
#[test]
fn actual_spawn_failure_removes_only_the_owned_namespace_and_preserves_inherited_ipc() {
    validate_source(&root()).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let binary = tmp.path().join("not-executable");
    fs::write(&binary, b"owned non-executable candidate\n").unwrap();
    let inherited = tempfile::Builder::new()
        .prefix("msgq_feedback-owner-")
        .tempdir_in("/dev/shm")
        .unwrap();
    fs::write(
        inherited.path().join("unrelated-sentinel"),
        b"must survive failure",
    )
    .unwrap();
    let name = inherited
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let output = tmp.path().join("spawn-failed");
    let observed = Command::new(checker())
        .arg("--binary")
        .arg(&binary)
        .arg("--output")
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", root())
        .env("OPENPILOT_PREFIX", name)
        .output()
        .unwrap();
    assert_eq!(observed.status.code(), Some(1));
    assert!(!output.join("comparison.json").exists());
    let failure: Value =
        serde_json::from_slice(&fs::read(output.join("native/lifecycle.json")).unwrap()).unwrap();
    assert_eq!(failure["phase"], "spawn");
    assert_eq!(failure["namespace_removed"], true);
    let owned = Path::new(failure["namespace"].as_str().unwrap());
    assert_ne!(owned, inherited.path());
    assert!(!owned.exists());
    assert_eq!(
        fs::read(inherited.path().join("unrelated-sentinel")).unwrap(),
        b"must survive failure"
    );
}
