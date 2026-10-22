#![cfg(target_os = "linux")]

use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use std::{env, path::PathBuf, time::Duration};

#[test]
#[ignore = "hosted Linux modem PTY, process-boundary, and live-daemon gate"]
fn modem_qa_replays_original_oracle_and_owned_native_processes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-modem"));
    let daemon = PathBuf::from(env!("CARGO_BIN_EXE_openpilot-modem"));
    let trace = env::var_os("MODEM_TRACE_BINARY")
        .map(PathBuf::from)
        .or_else(|| option_env!("CARGO_BIN_EXE_modem_trace").map(PathBuf::from))
        .expect("workflow supplies the built modem_trace example");
    let launcher = env::var_os("MODEM_PROCESS_CHILD_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| daemon.parent().unwrap().join("openpilot-process-child"));
    let evidence = tempfile::Builder::new()
        .prefix("modem-retirement-evidence-")
        .tempdir()
        .unwrap();
    let mut command = CommandSpec::new(
        vec![
            checker.to_string_lossy().into_owned(),
            "--binary".into(),
            daemon.to_string_lossy().into_owned(),
            "--trace".into(),
            trace.to_string_lossy().into_owned(),
            "--launcher".into(),
            launcher.to_string_lossy().into_owned(),
            "--evidence".into(),
            evidence.path().to_string_lossy().into_owned(),
            "--source-root".into(),
            root.to_string_lossy().into_owned(),
        ],
        Duration::from_secs(300),
    );
    if env::var_os("MODEM_REQUIRE_RETIRED").is_some_and(|value| value == "1") {
        command.argv.push("--require-retired".into());
    }
    let captured = NativeRunner.run(&command).unwrap();
    assert_eq!(
        captured.code,
        0,
        "{}",
        String::from_utf8_lossy(&captured.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&captured.stdout).unwrap();
    assert_eq!(report["passed"], 17);
    assert_eq!(report["scenarios"].as_array().unwrap().len(), 13);
    assert_eq!(report["process_boundary"].as_array().unwrap().len(), 3);
    for name in [
        "normal",
        "at_errors",
        "lpa_lock",
        "echo_retry",
        "identity_retry",
        "absent_sim",
        "roaming_blocked",
        "poll_errors",
        "retry_limit",
        "route_failure_retry",
        "route_failure",
        "invalid_route",
        "dns_failure",
    ] {
        assert!(evidence.path().join(format!("{name}.json")).is_file());
    }
    assert!(evidence.path().join("lifecycle.json").is_file());
    assert!(evidence.path().join("descriptor-boundary.json").is_file());
    assert!(evidence.path().join("shell-fallback.json").is_file());
    assert!(evidence.path().join("startup-order.json").is_file());
}
