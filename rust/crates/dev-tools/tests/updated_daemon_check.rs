//! Production daemon comparison against fresh or sealed original-only observations.
#![cfg(target_os = "linux")]
use chrono::{NaiveDateTime, TimeZone, Utc};
use openpilot_dev_tools::updated_daemon_check::{original_sha256, parse, run, Options};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::OsString,
    fs,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const SEALED_ORIGINAL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/updated-daemon-original-19e47ad9"
);
const SEALED_MANIFEST_SHA: &str =
    "214258ef372f065cf3f924e6d921cf4fd3258e801220504536c8d54e945094b3";
const SEALED_START_NS: i128 = 1792562491512826293;
const SEALED_END_NS: i128 = 1792562492873864873;

fn required(name: &str) -> PathBuf {
    PathBuf::from(
        env::var_os(name).unwrap_or_else(|| panic!("{name} required for daemon comparison")),
    )
}
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_owned()
}

fn owned_root(path: &Path) -> String {
    let bytes = fs::read_to_string(path).unwrap();
    let rows = bytes
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(!rows.is_empty(), "missing real overlay commands");
    let root = rows
        .iter()
        .flat_map(|row| row.as_array().unwrap())
        .filter_map(Value::as_str)
        .find_map(|arg| {
            let start = arg.find("/tmp/updated-daemon-")?;
            let end = start + arg[start..].find("/runtime/")? + "/runtime".len();
            Some(arg[start..end].to_owned())
        })
        .expect("overlay commands did not include owned runtime root");
    assert!(root.starts_with("/tmp/updated-daemon-"));
    root
}

fn overlay_commands(path: &Path) -> Vec<Value> {
    let owned_root = owned_root(path);
    let bytes = fs::read_to_string(path).unwrap();
    let rows = bytes
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    rows.into_iter()
        .map(|row| {
            Value::Array(
                row.as_array()
                    .unwrap()
                    .iter()
                    .map(|arg| Value::String(arg.as_str().unwrap().replace(&owned_root, "<ROOT>")))
                    .collect(),
            )
        })
        .collect()
}

fn now_ns() -> i128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i128
}

fn required_ns(name: &str) -> i128 {
    env::var(name)
        .unwrap_or_else(|_| panic!("{name} required for daemon comparison"))
        .parse()
        .unwrap_or_else(|_| panic!("{name} must be Unix nanoseconds"))
}

fn historical_source_absent() -> bool {
    match env::var("UPDATED_DAEMON_REQUIRE_ABSENT")
        .expect("explicit original controller state required")
        .as_str()
    {
        "0" => false,
        "1" => true,
        other => panic!("invalid UPDATED_DAEMON_REQUIRE_ABSENT: {other}"),
    }
}

fn sealed_original() -> PathBuf {
    let root = PathBuf::from(SEALED_ORIGINAL);
    let manifest = fs::read(root.join("SHA256SUMS")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&manifest)),
        SEALED_MANIFEST_SHA
    );
    let text = String::from_utf8(manifest).unwrap();
    let mut count = 0;
    for line in text.lines() {
        let (digest, name) = line.split_once("  ").unwrap();
        assert_eq!(digest.len(), 64);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root.join(name)).unwrap())),
            digest
        );
        count += 1;
    }
    assert_eq!(
        count, 8,
        "complete original production observation required"
    );
    root.join("original")
}

fn stable_params(value: &Value, root: &str, started_ns: i128, ended_ns: i128) -> Value {
    assert!(started_ns <= ended_ns, "invalid daemon observation window");
    let mut normalized = value.as_object().unwrap().clone();
    for value in normalized.values_mut() {
        let text = value.as_str().expect("original Params values must be text");
        *value = Value::String(text.replace(root, "<ROOT>"));
    }
    for key in ["UpdaterLastFetchTime", "LastUpdateTime", "InstallDate"] {
        if let Some(value) = normalized.get(key) {
            let text = value.as_str().unwrap();
            let date = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f")
                .unwrap_or_else(|_| panic!("unexpected {key} timestamp: {text}"));
            let timestamp_ns = Utc.from_utc_datetime(&date).timestamp_nanos_opt().unwrap() as i128;
            assert!(
                (started_ns..=ended_ns).contains(&timestamp_ns),
                "{key} timestamp outside its own daemon invocation: {text}"
            );
            normalized.insert(key.into(), Value::String("<ACTUAL_DATE>".into()));
        }
    }
    Value::Object(normalized)
}

fn fixture_parent(root: &str) -> &str {
    let parent = root.strip_suffix("/runtime").unwrap();
    let suffix = parent.strip_prefix("/tmp/updated-daemon-").unwrap();
    assert!(!suffix.is_empty() && !suffix.contains('/'));
    parent
}

fn record_sequences(records: &Value, parent: &str) -> BTreeMap<&'static str, Vec<Value>> {
    let rows = records.as_array().expect("log records must be an array");
    assert_eq!(rows.len(), 22, "original captured 22 production records");
    let signal_thread = rows
        .iter()
        .find(|row| row["msg"] == "caught SIGUSR1, checking for updates")
        .expect("missing SIGUSR1 log")["thread"]
        .as_u64()
        .expect("signal thread ID");
    let mut threads = BTreeSet::new();
    let mut sequences = BTreeMap::<&'static str, Vec<Value>>::new();
    let mut fetch_paths = 0;
    let mut timed_events = 0;
    for row in rows {
        let mut normalized = row.as_object().expect("log record object").clone();
        let thread = normalized["thread"].as_u64().expect("log thread ID");
        threads.insert(thread);
        let role = if thread == signal_thread {
            "signal"
        } else {
            "main"
        };
        let message = normalized.get_mut("msg").expect("log msg");
        if let Some(text) = message.as_str() {
            if text.starts_with("git fetch success: From ") {
                assert!(
                    text.contains(parent),
                    "fetch log omitted owned fixture parent"
                );
                *message = Value::String(text.replace(parent, "<FIXTURE_PARENT>"));
                fetch_paths += 1;
            } else {
                assert!(!text.contains(parent), "unexpected fixture path in log");
            }
        } else {
            let object = message.as_object_mut().expect("structured log msg");
            assert_eq!(object.len(), 2, "unexpected structured log fields");
            assert_eq!(object["event"], "Done LFS cleanup");
            let duration = object["duration"].as_f64().expect("LFS duration number");
            assert!(duration.is_finite() && (0.0..=15.0).contains(&duration));
            object.insert("duration".into(), Value::String("<DURATION>".into()));
            timed_events += 1;
        }
        for dynamic in ["host", "process", "thread", "created"] {
            assert!(
                normalized.remove(dynamic).is_some(),
                "missing {dynamic} field"
            );
        }
        sequences
            .entry(role)
            .or_default()
            .push(Value::Object(normalized));
    }
    assert_eq!(
        threads.len(),
        2,
        "expected main and signal producer threads"
    );
    assert_eq!(fetch_paths, 1, "expected one fixture-path fetch log");
    assert_eq!(timed_events, 1, "expected one LFS duration event");
    assert_eq!(sequences["signal"].len(), 2);
    assert_eq!(sequences["main"].len(), 20);
    sequences
}

#[test]
fn original_controller_identity_is_independent_and_unchanged() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/check_updated_daemon.py");
    if historical_source_absent() {
        assert!(
            matches!(fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
            "historical controller reappeared"
        );
        sealed_original();
    } else {
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(metadata.is_file(), "historical controller must be regular");
        let hash = format!("{:x}", Sha256::digest(fs::read(path).unwrap()));
        assert_eq!(hash, original_sha256());
    }
}

#[test]
fn argparse_help_abbreviation_and_repeated_option_keep_original_actions() {
    let exe = Path::new("/private/check-updated-daemon");
    for argv in [
        vec!["--binary"],
        vec!["--unknown", "value"],
        vec!["--binary", "first", "--launcher", "second"],
    ] {
        assert!(parse(
            &argv.into_iter().map(OsString::from).collect::<Vec<_>>(),
            exe
        )
        .is_err());
    }
    for argv in [vec!["-h"], vec!["--help"], vec!["-hh"]] {
        assert!(parse(
            &argv.into_iter().map(OsString::from).collect::<Vec<_>>(),
            exe
        )
        .unwrap()
        .is_none());
    }
    let args = [
        "--binary",
        "/tmp/binary",
        "--launcher",
        "/tmp/launcher",
        "--output",
        "/tmp/out",
    ]
    .map(OsString::from);
    let parsed = parse(&args, exe).unwrap().unwrap();
    assert_eq!(parsed.fixture, Path::new("/private/updated-fixture"));
    assert_eq!(parsed.binary, Path::new("/tmp/binary"));
    let repeated = [
        "--bin",
        "first",
        "--binary",
        "last",
        "--launch",
        "/tmp/launcher",
        "--out",
        "/tmp/out",
    ]
    .map(OsString::from);
    let parsed = parse(&repeated, exe).unwrap().unwrap();
    assert_eq!(parsed.binary, Path::new("last"));
    let raw = OsString::from_vec(b"/tmp/candidate-\xff".to_vec());
    let argv = [
        OsString::from("--binary"),
        raw.clone(),
        OsString::from("--launcher"),
        OsString::from("/tmp/launcher"),
        OsString::from("--output"),
        OsString::from("/tmp/out"),
    ];
    assert_eq!(
        parse(&argv, exe)
            .unwrap()
            .unwrap()
            .binary
            .as_os_str()
            .as_bytes(),
        raw.as_bytes()
    );
}

#[test]
fn missing_elf_and_foreign_output_reject_without_clobber() {
    let root = tempfile::tempdir().unwrap();
    let foreign = root.path().join("foreign");
    fs::write(&foreign, b"foreign receipt").unwrap();
    let options = Options {
        binary: PathBuf::from("/missing/updated"),
        launcher: PathBuf::from("/missing/launcher"),
        fixture: PathBuf::from("/missing/shim"),
        output: foreign.clone(),
        root: repo(),
    };
    assert!(run(&options).is_err());
    assert_eq!(fs::read(foreign).unwrap(), b"foreign receipt");
    let script = root.path().join("script");
    fs::write(&script, b"#!/bin/sh\nexit 0\n").unwrap();
    let fresh = root.path().join("fresh");
    let options = Options {
        binary: script,
        launcher: PathBuf::from("/bin/true"),
        fixture: PathBuf::from("/bin/true"),
        output: fresh.clone(),
        root: repo(),
    };
    assert!(run(&options).is_err());
    assert!(!fresh.exists());
}

#[test]
fn early_daemon_failure_retains_log_and_cannot_publish_success() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("failed");
    let options = Options {
        binary: PathBuf::from("/bin/false"),
        launcher: required("UPDATED_DAEMON_LAUNCHER"),
        fixture: required("UPDATED_DAEMON_FIXTURE"),
        output: output.clone(),
        root: repo(),
    };
    assert!(run(&options).is_err());
    assert!(output.join("daemon.log").is_file());
    assert!(!output.join("result.json").exists());
}

#[test]
fn native_production_daemon_matches_original_result_and_retains_raw_evidence() {
    let (original, original_started_ns, original_ended_ns) = if historical_source_absent() {
        (sealed_original(), SEALED_START_NS, SEALED_END_NS)
    } else {
        (
            required("UPDATED_DAEMON_ORIGINAL_OUTPUT"),
            required_ns("UPDATED_DAEMON_ORIGINAL_START_NS"),
            required_ns("UPDATED_DAEMON_ORIGINAL_END_NS"),
        )
    };
    let output = required("UPDATED_DAEMON_NATIVE_OUTPUT");
    let original_report: Value =
        serde_json::from_slice(&fs::read(original.join("result.json")).unwrap()).unwrap();
    assert_eq!(original_report["passed"], true);
    let options = Options {
        binary: required("UPDATED_DAEMON_BINARY"),
        launcher: required("UPDATED_DAEMON_LAUNCHER"),
        fixture: required("UPDATED_DAEMON_FIXTURE"),
        output: output.clone(),
        root: repo(),
    };
    let native_started_ns = now_ns();
    let native = run(&options).unwrap();
    let native_ended_ns = now_ns();
    let original_commit = original_report["finalized_commit"].as_str().unwrap();
    let native_commit = native["finalized_commit"].as_str().unwrap();
    for commit in [original_commit, native_commit] {
        assert_eq!(commit.len(), 40);
        assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
    assert_eq!(
        native, original_report,
        "finalized Git commit and full result"
    );
    assert_eq!(native["duplicate_lock_exit"], 1);
    assert_eq!(native["sigterm_exit"], 0);
    for name in [
        "daemon.log",
        "duplicate.log",
        "records.json",
        "params.json",
        "overlay-commands.jsonl",
        "result.json",
        "input-identities.json",
    ] {
        assert!(
            output.join(name).is_file(),
            "missing raw native evidence {name}"
        );
        if name != "input-identities.json" {
            assert!(
                original.join(name).is_file(),
                "missing raw original evidence {name}"
            );
        }
    }
    for name in ["daemon.log", "duplicate.log"] {
        assert_eq!(
            fs::read(output.join(name)).unwrap(),
            fs::read(original.join(name)).unwrap(),
            "raw production daemon observation changed: {name}"
        );
    }
    let records: Value =
        serde_json::from_slice(&fs::read(output.join("records.json")).unwrap()).unwrap();
    let original_records: Value =
        serde_json::from_slice(&fs::read(original.join("records.json")).unwrap()).unwrap();
    let native_root = owned_root(&output.join("overlay-commands.jsonl"));
    let original_root = owned_root(&original.join("overlay-commands.jsonl"));
    let native_sequences = record_sequences(&records, fixture_parent(&native_root));
    let original_sequences = record_sequences(&original_records, fixture_parent(&original_root));
    assert_eq!(native_sequences, original_sequences);
    assert_eq!(
        native_sequences["signal"]
            .iter()
            .map(|row| row["msg"].as_str().expect("signal msg string"))
            .collect::<Vec<_>>(),
        vec![
            "caught SIGUSR1, checking for updates",
            "caught SIGHUP, attempting to downloading update"
        ]
    );
    let params: Value =
        serde_json::from_slice(&fs::read(output.join("params.json")).unwrap()).unwrap();
    let original_params: Value =
        serde_json::from_slice(&fs::read(original.join("params.json")).unwrap()).unwrap();
    assert_eq!(
        stable_params(&params, &native_root, native_started_ns, native_ended_ns),
        stable_params(
            &original_params,
            &original_root,
            original_started_ns,
            original_ended_ns
        )
    );
    assert_eq!(
        overlay_commands(&output.join("overlay-commands.jsonl")),
        overlay_commands(&original.join("overlay-commands.jsonl"))
    );
    assert_eq!(params["UpdateAvailable"], "1");
    assert_eq!(params["UpdaterState"], "idle");
    assert_eq!(params["UpdaterFetchAvailable"], "0");
    assert!(fs::read(output.join("duplicate.log"))
        .unwrap()
        .windows(b"overlay lock".len())
        .any(|part| part == b"overlay lock"));
}
