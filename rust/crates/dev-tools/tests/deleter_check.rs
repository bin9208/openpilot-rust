#![cfg(all(target_os = "linux", feature = "deleter-check-native"))]
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use std::{fs, path::PathBuf, time::Duration};
fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("mandatory {name}"))
}
fn command(scope: &str) -> CommandSpec {
    CommandSpec::new(
        vec![
            env("DELETER_CHECKER"),
            "--root".into(),
            env("OPENPILOT_RUST_ROOT"),
            "--scope".into(),
            scope.into(),
            "--fixture".into(),
            env("DELETER_ORIGINAL_FIXTURE"),
            "--fixture-sha256".into(),
            env("DELETER_ORIGINAL_SHA256"),
            "--trace".into(),
            env("DELETER_TRACE"),
            "--binary".into(),
            env("DELETER_BINARY"),
            "--output".into(),
            PathBuf::from(env("DELETER_PROOF"))
                .join(scope)
                .to_string_lossy()
                .into_owned(),
        ],
        Duration::from_secs(120),
    )
}
#[test]
fn all_original_reference_rows_preserve_both_bookmark_histories() {
    let nested = PathBuf::from(env("DELETER_PROOF")).join("reference/reference");
    assert!(
        !nested.exists(),
        "reference starts with no nested output parent"
    );
    let output = NativeRunner.run(&command("reference")).unwrap();
    assert_eq!(
        output.code,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["reference_scenarios"], 28);
    assert_eq!(report["reference_comparisons"], 78);
    assert_eq!(report["original_and_corrected_rows_retained"], true);
    assert!(nested.join("threshold-order/trace.json").is_file());
}
#[test]
fn original_fixture_corruption_rejects_before_provider_start() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("bad.json");
    fs::write(&file, b"{}").unwrap();
    let mut spec = command("reference");
    let at = spec.argv.iter().position(|v| v == "--fixture").unwrap();
    spec.argv[at + 1] = file.to_string_lossy().into_owned();
    let output = NativeRunner.run(&spec).unwrap();
    assert_ne!(output.code, 0);
    assert!(String::from_utf8_lossy(&output.stderr).contains("original deleter fixture differs"));
}
#[test]
fn real_daemon_cli_idle_shutdown_and_private_low_space_follow_original_contract() {
    let output = NativeRunner.run(&command("daemon")).unwrap();
    assert_eq!(
        output.code,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["idle_and_cli_cases"], 6);
    assert_eq!(report["low_space_cycles"], 3);
    let details = PathBuf::from(env("DELETER_PROOF")).join("daemon/daemon/mount-details");
    let retained: serde_json::Value =
        serde_json::from_slice(&fs::read(details.join("low-space-observation.json")).unwrap())
            .unwrap();
    assert_eq!(retained, report["low_space"]);
    assert!(details.join("tmpfs-unmount/stderr.bin").is_file());
}

#[test]
fn merged_ready_and_unicode_first_delete_keep_all_three_lines_and_real_intervals() {
    let output = NativeRunner.run(&command("observer")).unwrap();
    assert_eq!(
        output.code,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["legacy_observer_regression_ported"], true);
}
