#![cfg(feature = "uploader-siblings-native")]
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use std::{fs, path::PathBuf, time::Duration};
fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("mandatory {key}"))
}
fn spec(scope: &str) -> CommandSpec {
    let mut spec = CommandSpec::new(
        vec![
            env("UPLOADER_SIBLING_CHECKER"),
            "--root".into(),
            env("OPENPILOT_RUST_ROOT"),
            "--scope".into(),
            scope.into(),
            "--fixture".into(),
            env("UPLOADER_SIBLING_ORIGINAL"),
            "--fixture-sha256".into(),
            "30f9e1f21c47cd9bcefcc161456e1df9984338cc2a2e940eabb8dce0a97a6176".into(),
            "--output".into(),
            PathBuf::from(env("UPLOADER_SIBLING_PROOF"))
                .join(scope)
                .to_string_lossy()
                .into_owned(),
        ],
        Duration::from_secs(300),
    );
    for (flag, key) in [
        ("--binary", "UPLOADER_BINARY"),
        ("--trace", "UPLOADER_TRACE_BINARY"),
        ("--fault", "UPLOADER_FAULT_BINARY"),
        ("--collector", "UPLOADER_COLLECTOR_BINARY"),
        ("--interrupted", "UPLOADER_INTERRUPTED_BINARY"),
        ("--http-provider", "UPLOADER_HTTP_PROVIDER"),
    ] {
        spec.argv.extend([flag.into(), env(key)]);
    }
    spec
}
fn run(scope: &str) -> Value {
    let spec = spec(scope);
    let captured = NativeRunner.run(&spec).unwrap();
    let output = PathBuf::from(env("UPLOADER_SIBLING_PROOF")).join(scope);
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("cli.stdout"), &captured.stdout).unwrap();
    fs::write(output.join("cli.stderr"), &captured.stderr).unwrap();
    assert_eq!(
        captured.code,
        0,
        "scope {scope}: {}",
        String::from_utf8_lossy(&captured.stderr)
    );
    let value: Value = serde_json::from_slice(&captured.stdout).unwrap();
    assert_eq!(value["passed"], true);
    assert_eq!(value["fresh_python_runtime"], false);
    value
}
#[test]
fn corrupted_original_observations_are_rejected_before_native_providers() {
    let temp = tempfile::tempdir().unwrap();
    let bad = temp.path().join("corrupt.json");
    fs::write(&bad, b"{}").unwrap();
    let mut command = spec("cleanup");
    let index = command.argv.iter().position(|v| v == "--fixture").unwrap();
    command.argv[index + 1] = bad.to_string_lossy().into_owned();
    let output = NativeRunner.run(&command).unwrap();
    assert_ne!(output.code, 0);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("sibling original observations changed")
    );
}
#[test]
fn six_actual_daemon_cases_and_three_startup_failures() {
    let v = run("daemon");
    assert_eq!(
        v["reports"]["daemon"]["scenarios"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        v["reports"]["daemon"]["startup_errors"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn interrupted_send_retries_preserve_actual_packet_and_delivery() {
    let v = run("interrupted-send");
    assert_eq!(v["reports"]["interrupted-send"]["interrupted_attempts"], 3);
}
#[test]
fn interrupted_http_read_retains_two_real_uploads() {
    run("interrupted-http");
}
#[test]
fn ten_socket_deadline_and_progress_cases_match_original() {
    let v = run("timeouts");
    assert_eq!(v["reports"]["timeouts"]["executions"], 10);
}
#[test]
fn fifteen_fault_cases_cross_both_actual_collectors() {
    let v = run("fault");
    assert_eq!(v["reports"]["fault"]["native_executions"], 30);
}
#[test]
fn seven_typed_params_cases_cross_both_actual_collectors() {
    let v = run("params");
    assert_eq!(v["reports"]["params"]["native_executions"], 14);
}
#[test]
fn real_timeout_and_early_spawn_cleanup_preserve_unrelated_resources() {
    let v = run("cleanup");
    assert_eq!(
        v["reports"]["cleanup"]["scenarios"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
