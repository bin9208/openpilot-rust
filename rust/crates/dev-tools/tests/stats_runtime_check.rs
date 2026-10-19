//! Source assertions, not candidate-derived expected runtime results.
#![cfg(feature = "stats-daemon-native")]
use openpilot_dev_tools::stats_daemon_check::runtime::{self as checker, Options};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

static ENDPOINT: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(target_os = "linux")]
mod lifecycle {
    use super::*;
    include!("stats_runtime_lifecycle.rs.inc");
}

fn options(output: &Path) -> Options {
    Options {
        root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        binaries: std::env::var_os("STATS_RUNTIME_BINARIES")
            .expect("fresh sealed production statsd-rs and stats_flood")
            .into(),
        output: output.into(),
    }
}
fn worker(name: &str) -> PathBuf {
    std::env::var_os(name)
        .expect("fresh native QA checker")
        .into()
}

#[test]
fn original_runtime_and_flood_assertion_sources_are_unchanged() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let pins: Value =
        serde_json::from_str(include_str!("fixtures/stats-runtime/source-pins.json")).unwrap();
    assert_eq!(pins.as_object().unwrap().len(), 17);
    for (path, expected) in pins.as_object().unwrap() {
        let input = root.join(path);
        if matches!(
            path.as_str(),
            "rust/tools/check_stats_runtime.py" | "rust/tools/check_stats_flood.py"
        ) && fs::symlink_metadata(&input)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            continue;
        }
        let bytes = fs::read(input).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            expected.as_str().unwrap(),
            "{path}"
        );
    }
}

#[test]
fn runtime_validator_keeps_real_clock_file_and_shutdown_assertions() {
    // A synthetic observation satisfying the independently read original
    // predicates; these are not captured answers from a native candidate.
    let valid = json!({
        "result":"PASS", "before_ns":100, "file_timestamp_ns":150, "after_ns":200,
        "file":"gauge.runtime,started=True,version=v value=7.0,dongle_id=\"None\" 150\n",
        "shutdown_signal":"SIGTERM", "exit":0
    });
    checker::validate_runtime(&valid, 1, 0o600).unwrap();
    let mut false_started = valid.clone();
    false_started["file"] =
        json!("gauge.runtime,started=False,version=v value=7.0,dongle_id=\"None\" 150\n");
    checker::validate_runtime(&false_started, 1, 0o600).unwrap();
    for (count, mode) in [(0, 0o600), (2, 0o600), (1, 0o644)] {
        assert!(checker::validate_runtime(&valid, count, mode).is_err());
    }
    for (key, value) in [
        ("before_ns", json!(151)),
        ("after_ns", json!(149)),
        ("exit", json!(1)),
        (
            "file",
            json!("gauge.other,started=True value=7.0,dongle_id=\"None\" 150\n"),
        ),
        (
            "file",
            json!("gauge.runtime,started=True value=8.0,dongle_id=\"None\" 150\n"),
        ),
        (
            "file",
            json!("gauge.runtime,started=True value=7.0,dongle_id=\"None\" not-a-timestamp\n"),
        ),
    ] {
        let mut wrong = valid.clone();
        wrong[key] = value;
        assert!(
            checker::validate_runtime(&wrong, 1, 0o600).is_err(),
            "{key}: {wrong}"
        );
    }
}

#[test]
fn flood_validator_requires_busy_daemon_live_producers_and_no_shutdown_flush() {
    for signal in [2, 15] {
        let valid = json!({"signal":signal,"exit":0,"shutdown_seconds":0.01,
            "busy_cpu_ticks":10,"producers_alive":true});
        checker::validate_flood(&valid, 0).unwrap();
        assert!(checker::validate_flood(&valid, 1).is_err());
        for (key, value) in [
            ("exit", json!(null)),
            ("exit", json!(1)),
            ("busy_cpu_ticks", json!(9)),
            ("producers_alive", json!(false)),
        ] {
            let mut wrong = valid.clone();
            wrong[key] = value;
            assert!(
                checker::validate_flood(&wrong, 0).is_err(),
                "{key}: {wrong}"
            );
        }
    }
}

// These two positive process checks share the production literal /tmp/stats.
// Keep them in one test so the test harness cannot run them concurrently.
#[test]
fn actual_production_clock_publication_and_two_busy_shutdown_signals() {
    let _guard = ENDPOINT.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let runtime = checker::run_runtime(
        &options(&temp.path().join("runtime")),
        &worker("STATS_RUNTIME_CHECKER"),
    )
    .unwrap();
    let runtime: Value = serde_json::from_str(&runtime.to_json().unwrap()).unwrap();
    assert_eq!(runtime["result"], "PASS");
    assert_eq!(runtime["shutdown_signal"], "SIGTERM");
    assert_eq!(runtime["exit"], 0);
    let flood_output = temp.path().join("flood");
    // Unlike runtime's mkdir, the original flood checker creates only the
    // per-signal children and accepts an existing root. Keep unrelated files.
    fs::create_dir(&flood_output).unwrap();
    fs::write(flood_output.join("sentinel"), b"foreign output").unwrap();
    let flood =
        checker::run_flood(&options(&flood_output), &worker("STATS_FLOOD_CHECKER")).unwrap();
    let flood: Value = serde_json::from_str(&flood.to_json().unwrap()).unwrap();
    assert_eq!(flood["result"], "PASS");
    let cases = flood["scenarios"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0]["signal"], 2);
    assert_eq!(cases[1]["signal"], 15);
    assert_eq!(
        fs::read(flood_output.join("sentinel")).unwrap(),
        b"foreign output"
    );
    for row in cases {
        checker::validate_flood(row, 0).unwrap();
    }
}

#[test]
fn missing_production_candidate_cannot_create_success_receipts() {
    let _guard = ENDPOINT.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let mut options = options(&temp.path().join("runtime"));
    options.binaries = temp.path().join("missing");
    assert!(checker::run_runtime(&options, &worker("STATS_RUNTIME_CHECKER")).is_err());
    assert!(!options.output.join("result.json").exists());
    options.output = temp.path().join("flood");
    assert!(checker::run_flood(&options, &worker("STATS_FLOOD_CHECKER")).is_err());
    assert!(!options.output.join("comparison.json").exists());
}

#[test]
fn only_exact_two_retired_sources_may_be_absent_and_present_inputs_remain_pinned() {
    use std::os::unix::fs::symlink;
    let _guard = ENDPOINT.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let pins: std::collections::BTreeMap<String, String> =
        serde_json::from_str(include_str!("fixtures/stats-runtime/source-pins.json")).unwrap();
    let root = temp.path().join("repository");
    let retired = [
        "rust/tools/check_stats_runtime.py",
        "rust/tools/check_stats_flood.py",
    ];
    let mut originals = std::collections::BTreeMap::new();
    for name in pins.keys() {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = repository.join(name);
        let bytes = match fs::symlink_metadata(&original) {
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && retired.contains(&name.as_str()) =>
            {
                None
            }
            _ => Some(fs::read(original).unwrap()),
        };
        if let Some(bytes) = &bytes {
            fs::write(path, bytes).unwrap();
        }
        originals.insert(name.clone(), bytes);
    }
    let mut sequence = 0;
    let mut check = |admitted: bool| {
        sequence += 1;
        let output = temp.path().join(format!("case-{sequence}"));
        let mut configured = options(&output);
        configured.root = root.clone();
        configured.binaries = temp.path().join("missing-binaries");
        let error = checker::run_runtime(&configured, &worker("STATS_RUNTIME_CHECKER"))
            .err()
            .expect("missing candidate must reject without success");
        assert_eq!(
            output.join("source/build.json").is_file(),
            admitted,
            "{error}"
        );
        assert!(!output.join("result.json").exists());
    };
    for name in retired {
        if let Some(bytes) = &originals[name] {
            fs::remove_file(root.join(name)).unwrap();
            check(true);
            fs::write(root.join(name), bytes).unwrap();
        }
    }
    for name in retired {
        if root.join(name).exists() {
            fs::remove_file(root.join(name)).unwrap();
        }
    }
    check(true);
    for name in retired {
        if let Some(bytes) = &originals[name] {
            fs::write(root.join(name), bytes).unwrap();
        }
    }
    for name in pins.keys() {
        let path = root.join(name);
        fs::write(&path, b"changed original").unwrap();
        check(false);
        fs::remove_file(&path).unwrap();
        symlink("missing-source-target", &path).unwrap();
        check(false);
        fs::remove_file(&path).unwrap();
        if !retired.contains(&name.as_str()) {
            check(false);
        }
        fs::create_dir(&path).unwrap();
        check(false);
        fs::remove_dir(&path).unwrap();
        if let Some(bytes) = &originals[name] {
            fs::write(&path, bytes).unwrap();
        }
    }
}
