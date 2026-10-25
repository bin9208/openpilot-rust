//! Real child/filesystem/transport checks against unchanged source captures.
#![cfg(feature = "stats-daemon-native")]
use openpilot_dev_tools::stats_daemon_check::{self as checker, Options};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[path = "stats_daemon_check/lifecycle.rs"]
mod lifecycle;
fn original() -> Value {
    let bytes = include_bytes!("fixtures/stats-daemon/original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "6376c0a5e990c4040ffa115cde2fd357810c760a32a85dc2ba9f49c4eb2a57c1"
    );
    serde_json::from_slice(bytes).unwrap()
}
fn configured(output: &Path) -> (Options, PathBuf) {
    let options = Options {
        root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        binaries: std::env::var_os("STATS_DAEMON_BINARIES")
            .expect("fresh daemon examples")
            .into(),
        binding: std::env::var_os("STATS_DAEMON_BINDING")
            .expect("original Params provenance")
            .into(),
        output: output.to_owned(),
    };
    (
        options,
        std::env::var_os("STATS_DAEMON_CHECKER")
            .expect("fresh native checker")
            .into(),
    )
}
#[test]
fn real_clock_flush_capacity_and_all_original_failure_effects_match() {
    let root = tempfile::tempdir().unwrap();
    let expected = original();
    for case in [
        "daemon",
        "utf8",
        "surrogate",
        "missing-directory",
        "permission",
        "collision",
        "dangling",
        "missing-param",
        "invalid-param",
        "log-fault",
    ] {
        let (options, worker) = configured(&root.path().join(case));
        let result = checker::run_case(&options, case, &worker).unwrap();
        assert_eq!(result["result"], "PASS", "{case}");
        assert_eq!(result["reference_kind"], "captured unchanged original");
        assert_eq!(result["native"], expected[case], "{case}");
    }
}
#[test]
fn existing_receipt_is_not_replaced_or_deleted() {
    let root = tempfile::tempdir().unwrap();
    let (options, worker) = configured(root.path());
    fs::write(root.path().join("result.json"), b"foreign").unwrap();
    assert!(checker::run_case(&options, "log-fault", &worker).is_err());
    assert_eq!(
        fs::read(root.path().join("result.json")).unwrap(),
        b"foreign"
    );
}
#[test]
fn missing_candidate_cannot_leave_success() {
    let root = tempfile::tempdir().unwrap();
    let (mut options, worker) = configured(&root.path().join("output"));
    options.binaries = root.path().join("absent");
    assert!(checker::run_case(&options, "daemon", &worker).is_err());
    assert!(!options.output.join("result.json").exists());
}
#[test]
fn malformed_clock_peer_is_rejected_without_success() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let (mut options, worker) = configured(&root.path().join("output"));
    options.binaries = root.path().join("candidate");
    let examples = options.binaries.join("examples");
    fs::create_dir_all(&examples).unwrap();
    fs::write(
        examples.join("stats_daemon"),
        b"#!/bin/sh\nprintf 'wrong-clock\\n'\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(
        examples.join("stats_daemon"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(examples.join("stats_producer"), b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(
        examples.join("stats_producer"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let error = checker::run_case(&options, "log-fault", &worker)
        .unwrap_err()
        .to_string();
    assert!(error.contains("clock"), "{error}");
    assert!(!options.output.join("result.json").exists());
}
