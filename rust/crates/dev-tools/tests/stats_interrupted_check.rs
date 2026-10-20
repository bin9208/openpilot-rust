//! Independent current original captures precede the native controller.
#![cfg(feature = "stats-daemon-native")]
#[cfg(target_os = "linux")]
#[path = "stats_interrupted_check/lifecycle.rs"]
mod lifecycle;
use openpilot_dev_tools::stats_daemon_check::interrupted::{self as checker, Options};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[test]
fn mandatory_ci_uses_native_controller_and_real_receive_fault_wrapper() {
    let workflow = include_str!("../../../../.github/workflows/rust.yml");
    let step = workflow
        .split("      - name: Compare interrupted metric receives with the original daemon\n")
        .nth(1)
        .unwrap()
        .split("      - name:")
        .next()
        .unwrap();
    for required in [
        "--features stats-daemon-native --bin check-stats-interrupted --locked",
        "-DZMQ_LINK_WRAP -c rust/tools/zmq_receive_boundary.c",
        "-shared rust/tools/zmq_receive_boundary.c -ldl",
        "-C link-arg=-Wl,--wrap=zmq_msg_recv",
        "STATS_INTERRUPTED_BINARY=",
        "STATS_INTERRUPTED_BINDING=",
        "STATS_INTERRUPTED_PRELOAD=",
        "STATS_INTERRUPTED_CHECKER=",
        "--features stats-daemon-native --lib --test stats_interrupted_check --locked",
        "cargo clippy",
        "--lib --test stats_interrupted_check --bin check-stats-interrupted --locked -- -D warnings",
        "rust/target/debug/check-stats-interrupted rust/target/debug/examples/stats_daemon",
    ] {
        assert!(step.contains(required), "missing mandatory receive gate: {required}");
    }
    assert!(!step.contains("python rust/tools/check_stats_interrupted.py"));
    assert_eq!(step.matches("rust/tools/check-disk-space").count(), 3);
}
fn rows() -> Vec<Value> {
    let bytes = include_bytes!("fixtures/stats-interrupted/original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "30af65083d9293acd0791f11d2ce40cb8eb611d2a7b4211dc82601d91e69020a"
    );
    serde_json::from_slice(bytes).unwrap()
}
fn options(output: &Path) -> Options {
    Options {
        root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        binary: std::env::var_os("STATS_INTERRUPTED_BINARY")
            .expect("fresh fault-wrapped native daemon")
            .into(),
        binding: std::env::var_os("STATS_INTERRUPTED_BINDING")
            .expect("source Params provenance")
            .into(),
        preload: std::env::var_os("STATS_INTERRUPTED_PRELOAD")
            .expect("fresh original fault shim")
            .into(),
        output: output.to_owned(),
        before: false,
    }
}
fn worker() -> PathBuf {
    std::env::var_os("STATS_INTERRUPTED_CHECKER")
        .expect("fresh native checker")
        .into()
}
#[test]
fn exact_original_results_and_before_failure_contract_remain_distinct() {
    let expected = rows();
    assert_eq!(expected.len(), 5);
    for row in expected {
        let case = row["case"].as_str().unwrap();
        checker::compare_row(case, &row["result"], false).unwrap();
        // Explicit historical-bug mode accepts any failing exit with only the
        // first EINTR recorded and no flush; it must never accept a fixed run.
        checker::compare_row(case, &json!({"exit":7,"files":[],"faults":[4]}), true).unwrap();
        assert!(
            checker::compare_row(case, &json!({"exit":0,"files":[],"faults":[4]}), true).is_err()
        );
        assert!(checker::compare_row(
            case,
            &json!({"exit":1,"files":["unexpected"],"faults":[4]}),
            true
        )
        .is_err());
        let mut wrong = row["result"].clone();
        wrong["faults"] = json!([99]);
        assert!(checker::compare_row(case, &wrong, false).is_err());
    }
}
#[test]
fn actual_wrapped_receive_faults_preserve_original_flush_and_fatal_results() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(&temp.path().join("result"));
    let result = checker::run(&options, &worker()).unwrap();
    assert_eq!(result["before"], false);
    assert_eq!(result["reference_kind"], "captured unchanged original");
    let result = result["results"].as_array().unwrap();
    assert_eq!(result.len(), 5);
    for (actual, original) in result.iter().zip(rows()) {
        assert_eq!(actual["case"], original["case"]);
        assert_eq!(actual["source"], original["result"]);
        assert_eq!(actual["native"], original["result"]);
    }
}
#[test]
fn missing_candidate_does_not_create_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let mut options = options(&temp.path().join("result"));
    options.binary = temp.path().join("missing");
    assert!(checker::run(&options, &worker()).is_err());
    assert!(!options.output.join("result.json").exists());
}
#[test]
fn preexisting_foreign_output_is_unchanged_on_rejection() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(temp.path());
    let receipt = temp.path().join("result.json");
    fs::write(&receipt, b"foreign").unwrap();
    assert!(checker::run(&options, &worker()).is_err());
    assert_eq!(fs::read(receipt).unwrap(), b"foreign");
}
