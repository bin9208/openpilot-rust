//! Collector failure stages captured from unchanged check_version_collector.py.
//! Private pending API RED. No original/candidate policy is implemented here.
#![cfg(feature = "version-tools-native")]

use openpilot_dev_tools::version_collector_check as collector;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fd::OwnedFd,
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn environment() -> BTreeMap<OsString, OsString> {
    std::env::vars_os().collect()
}
fn native(name: &str) -> PathBuf {
    let path = std::env::var_os(name).unwrap_or_else(|| {
        panic!("{name} must name the current source-matched native ELF; prerequisite missing")
    });
    let path = fs::canonicalize(path).unwrap();
    assert!(fs::read(&path).unwrap().starts_with(b"\x7fELF"), "{name}");
    path
}
fn options(probe: &Path, daemon: &Path, output: &Path) -> collector::Options {
    collector::parse(
        &[
            probe.as_os_str().to_owned(),
            daemon.as_os_str().to_owned(),
            output.as_os_str().to_owned(),
        ],
        &root(),
        &environment(),
    )
    .unwrap()
}
fn original() -> Value {
    // Promote this immutable capture beside the test at parent registration.
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/version-tools/collector-boundaries-original.json"),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "37a1036463b262139a5ad1c244b18aeed52b88d16fe6e352d1c73c0d0961b0a6"
    );
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["cases"].as_array().unwrap().len(), 10);
    value
}
fn case<'a>(original: &'a Value, name: &str) -> &'a Value {
    original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == name)
        .unwrap()
}
fn failure_files(output: &Path, original: &Value) {
    assert!(output.is_dir());
    assert!(output.join("collector/stdout.log").is_file());
    assert!(output.join("collector/stderr.log").is_file());
    for name in [
        "probe.stdout",
        "probe.stderr",
        "source.json",
        "collector/invocation.json",
    ] {
        assert_eq!(
            output.join(name).is_file(),
            original["artifacts"].get(name).is_some(),
            "failure stage changed: {name}"
        );
    }
    for name in [
        "manifest.json",
        "records.json",
        "logMessage.bin",
        "errorLogMessage.bin",
    ] {
        assert!(
            !output.join(name).exists(),
            "premature success artifact: {name}"
        );
    }
    // The original zero/empty probe persists all three exact early files, not
    // just their presence. Native daemon output is never an expected oracle.
    for name in ["probe.stdout", "probe.stderr", "source.json"] {
        if let Some(hash) = original["artifacts"].get(name) {
            assert_eq!(
                format!("{:x}", Sha256::digest(fs::read(output.join(name)).unwrap())),
                hash.as_str().unwrap(),
                "{name}"
            );
        }
    }
}
fn script(path: &Path, bytes: &[u8]) -> PathBuf {
    let mut body = path.as_os_str().to_owned();
    body.push(".body");
    let body = PathBuf::from(body);
    fs::write(&body, bytes).unwrap();
    let mut permissions = fs::metadata(&body).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&body, permissions).unwrap();
    symlink(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/version-tools/collector-launcher"),
        path,
    )
    .unwrap();
    body
}
fn prefix_clean(prefix: &str) {
    assert!(!prefix.is_empty());
    assert!(prefix
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    assert!(!Path::new("/dev/shm")
        .join(format!("msgq_{prefix}"))
        .exists());
    assert!(!Path::new("/tmp")
        .join(format!("logmessage{prefix}"))
        .exists());
}

#[test]
fn all_four_original_collector_arity_failures_precede_output_and_worker_effects() {
    let original = original();
    let directory = tempfile::tempdir().unwrap();
    let before_env = environment();
    let before_cwd = std::env::current_dir().unwrap();
    for count in [0, 1, 2, 4] {
        let row = case(&original, &format!("arity-{count}"));
        assert_eq!(row["exit"], 1);
        assert_eq!(row["output_exists"], false);
        let args = vec![OsString::from("absent"); count];
        assert!(collector::parse(&args, directory.path(), &before_env).is_err());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }
    assert_eq!(std::env::current_dir().unwrap(), before_cwd);
    assert_eq!(environment(), before_env);
}

#[test]
fn original_missing_and_early_collector_fail_before_probe_artifacts_and_clean_their_namespace() {
    let original = original();
    let probe = native("VERSION_PROBE_BINARY");
    let worker = native("VERSION_WORKER_BINARY");
    let directory = tempfile::tempdir().unwrap();
    let before_env = environment();
    for (name, daemon) in [
        (
            "missing-collector",
            directory.path().join("absent-collector"),
        ),
        ("collector-early-zero", PathBuf::from("/bin/true")),
        ("collector-early-nonzero", PathBuf::from("/bin/false")),
    ] {
        let output = directory.path().join(name);
        let marker = directory.path().join(format!("{name}.prefix"));
        let mut options = options(&probe, &daemon, &output);
        // Passive wrapper records the actual namespace, then execs the same
        // original exit0/exit1 fixture. Missing-exec is exercised without wrapping.
        if name != "missing-collector" {
            let wrapper = directory.path().join(format!("{name}.sh"));
            let body = script(&wrapper, b"#!/bin/sh\nprintf '%s\\n' \"$OPENPILOT_PREFIX\" > \"$VERSION_TEST_PREFIX_MARKER\"\nexec \"$VERSION_TEST_REAL_COLLECTOR\"\n");
            options.collector = wrapper;
            options
                .environment
                .insert("VERSION_COLLECTOR_TEST_BODY".into(), body.into_os_string());
            options.environment.insert(
                "VERSION_TEST_PREFIX_MARKER".into(),
                marker.as_os_str().to_owned(),
            );
            options.environment.insert(
                "VERSION_TEST_REAL_COLLECTOR".into(),
                daemon.into_os_string(),
            );
        }
        let started = Instant::now();
        assert!(
            collector::run(&root(), &options, &worker).is_err(),
            "{name}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "early exit consumed readiness deadline: {name}"
        );
        failure_files(&output, case(&original, name));
        if name != "missing-collector" {
            prefix_clean(fs::read_to_string(marker).unwrap().trim());
        }
    }
    assert_eq!(environment(), before_env);
}

// pidfd pins the live fixture identity before its acknowledged transition into
// the actual daemon. No test cleanup ever signals a stored/recycled numeric PID.
struct DaemonOwner(OwnedFd);
impl DaemonOwner {
    fn exited(&self, timeout: Duration) -> bool {
        let mut descriptors = [PollFd::new(&self.0, PollFlags::IN)];
        poll(
            &mut descriptors,
            Some(&Timespec::try_from(timeout).unwrap()),
        )
        .unwrap()
            == 1
    }
}
impl Drop for DaemonOwner {
    fn drop(&mut self) {
        let _ = pidfd_send_signal(&self.0, Signal::KILL);
    }
}
fn observe(marker: PathBuf, ack: PathBuf) -> thread::JoinHandle<DaemonOwner> {
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(text) = fs::read_to_string(&marker) {
                if let Ok(raw) = text.trim().parse::<i32>() {
                    if let Some(pid) = Pid::from_raw(raw) {
                        if let Ok(fd) = pidfd_open(pid, PidfdFlags::empty()) {
                            let owner = DaemonOwner(fd);
                            assert!(!owner.exited(Duration::ZERO), "fixture was not live");
                            fs::write(ack, b"identity held before actual daemon exec\n").unwrap();
                            return owner;
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("collector worker never reached actual owned daemon startup");
    })
}

#[test]
fn three_original_probe_failures_keep_exact_artifact_phase_and_reap_the_actual_collector() {
    let original = original();
    let daemon = native("VERSION_COLLECTOR_BINARY");
    let worker = native("VERSION_WORKER_BINARY");
    let directory = tempfile::tempdir().unwrap();
    let before_env = environment();
    for (name, probe) in [
        ("missing-probe", directory.path().join("absent-probe")),
        ("probe-early-zero", PathBuf::from("/bin/true")),
        ("probe-early-nonzero", PathBuf::from("/bin/false")),
    ] {
        let case_dir = directory.path().join(name);
        fs::create_dir(&case_dir).unwrap();
        let output = case_dir.join("evidence");
        let marker = case_dir.join("daemon.pid");
        let prefix = case_dir.join("daemon.prefix");
        let ack = case_dir.join("observed");
        let wrapper = case_dir.join("actual-collector.sh");
        let body = script(&wrapper,b"#!/bin/sh\nprintf '%s\\n' \"$OPENPILOT_PREFIX\" > \"$VERSION_TEST_PREFIX_MARKER\"\nprintf '%s\\n' \"$$\" > \"$VERSION_TEST_PID_MARKER\"\nwhile [ ! -f \"$VERSION_TEST_ACK\" ]; do /bin/sleep 0.01; done\nexec \"$VERSION_TEST_REAL_COLLECTOR\"\n");
        let observer = observe(marker.clone(), ack.clone());
        let mut options = options(&probe, &wrapper, &output);
        options.environment.extend([
            ("VERSION_COLLECTOR_TEST_BODY".into(), body.into_os_string()),
            (
                "VERSION_TEST_PREFIX_MARKER".into(),
                prefix.as_os_str().to_owned(),
            ),
            ("VERSION_TEST_PID_MARKER".into(), marker.into_os_string()),
            ("VERSION_TEST_ACK".into(), ack.into_os_string()),
            (
                "VERSION_TEST_REAL_COLLECTOR".into(),
                daemon.as_os_str().to_owned(),
            ),
        ]);
        let result = collector::run(&root(), &options, &worker);
        let observed = observer.join().unwrap();
        assert!(result.is_err(), "{name}");
        assert!(
            observed.exited(Duration::from_secs(1)),
            "actual collector survived {name}"
        );
        failure_files(&output, case(&original, name));
        let prefix = fs::read_to_string(prefix).unwrap();
        let invocation: Value =
            serde_json::from_slice(&fs::read(output.join("collector/invocation.json")).unwrap())
                .unwrap();
        assert_eq!(invocation["OPENPILOT_PREFIX"], prefix.trim());
        prefix_clean(prefix.trim());
    }
    assert_eq!(environment(), before_env);
}

#[test]
fn existing_output_file_and_stale_collector_directory_are_preserved_on_failure() {
    let probe = native("VERSION_PROBE_BINARY");
    let daemon = native("VERSION_COLLECTOR_BINARY");
    let worker = native("VERSION_WORKER_BINARY");
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("file");
    fs::write(&file, b"existing caller-owned bytes\n").unwrap();
    assert!(collector::run(&root(), &options(&probe, &daemon, &file), &worker).is_err());
    assert_eq!(fs::read(&file).unwrap(), b"existing caller-owned bytes\n");
    let output = directory.path().join("old-output");
    fs::create_dir_all(output.join("collector")).unwrap();
    fs::write(
        output.join("collector/marker"),
        b"not owned by the current invocation\n",
    )
    .unwrap();
    assert!(collector::run(&root(), &options(&probe, &daemon, &output), &worker).is_err());
    assert_eq!(
        fs::read(output.join("collector/marker")).unwrap(),
        b"not owned by the current invocation\n"
    );
    assert!(!output.join("manifest.json").exists());
}
