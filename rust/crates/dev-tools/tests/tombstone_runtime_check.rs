//! Original actual daemon/collector/filesystem/HTTP assertions, without a candidate oracle.
#![cfg(all(feature = "tombstone-tools-native", feature = "sdk-receiver-native"))]
use openpilot_dev_tools::{seal::digest, tombstone_runtime_check as runtime};
use openpilot_log_format::{JsonValue as J, JsonView};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
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
fn artifact(key: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(key)
            .unwrap_or_else(|| panic!("{key} must name a current verified native artifact")),
    );
    let path = path.canonicalize().unwrap();
    assert!(fs::read(&path).unwrap().starts_with(b"\x7fELF"));
    path
}
fn fixture() -> J {
    let bytes = include_bytes!("fixtures/tombstone-live/original-contracts.json");
    use sha2::{Digest, Sha256};
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "e0c391f3d751fd7e8c26445026ac03a65693ba947ae2290977d50ec8c578beab"
    );
    J::parse(std::str::from_utf8(bytes).unwrap()).unwrap()
}
#[test]
fn unchanged_original_runtime_and_shutdown_contracts_are_pinned() {
    let original = fixture();
    let provenance = runtime::source_provenance(&root()).unwrap();
    let source_pins = original.get("source_sha256").unwrap();
    let JsonView::Object(pins) = source_pins.view() else {
        panic!("source pins must be a map")
    };
    for (name, expected) in pins {
        let name = name
            .iter()
            .map(|p| char::from_u32(*p).unwrap())
            .collect::<String>();
        if let Some(historical) = provenance
            .get("historical_source_sha256")
            .unwrap()
            .get(&name)
        {
            assert_eq!(historical.to_json().unwrap(), expected.to_json().unwrap());
        } else {
            assert_eq!(
                digest(&root().join(&name)).unwrap(),
                expected.to_utf8().unwrap(),
                "{name}"
            );
        }
    }
    assert_eq!(
        original
            .get("runtime")
            .unwrap()
            .get("case_count")
            .unwrap()
            .to_json()
            .unwrap(),
        "2"
    );
    assert_eq!(
        original
            .get("runtime")
            .unwrap()
            .get("startup_failure_cases")
            .unwrap()
            .to_json()
            .unwrap(),
        "4"
    );
}

#[test]
fn retired_live_checkers_are_historical_but_changed_dangling_and_missing_policy_inputs_reject() {
    use std::os::unix::fs::symlink;
    let original: serde_json::Value = serde_json::from_str(
        &runtime::source_provenance(&root())
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    let copied = tempfile::tempdir().unwrap();
    for name in original["source_sha256"].as_object().unwrap().keys() {
        let target = copied.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(name), target).unwrap();
    }
    let retired = [
        "rust/tools/check_tombstoned_runtime.py",
        "rust/tools/check_tombstoned_shutdown.py",
    ];
    for name in retired {
        let file = copied.path().join(name);
        if file.exists() {
            fs::remove_file(file).unwrap();
        }
    }
    let report: serde_json::Value = serde_json::from_str(
        &runtime::source_provenance(copied.path())
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        report["historical_source_sha256"]
            .as_object()
            .unwrap()
            .len(),
        original["historical_source_sha256"]
            .as_object()
            .unwrap()
            .len()
            + retired
                .iter()
                .filter(|name| original["source_sha256"].get(**name).is_some())
                .count()
    );
    for name in retired {
        assert!(report["source_sha256"].get(name).is_none());
        let target = copied.path().join(name);
        fs::write(&target, b"changed source").unwrap();
        assert!(runtime::source_provenance(copied.path()).is_err());
        fs::remove_file(&target).unwrap();
        symlink("missing-original", &target).unwrap();
        assert!(runtime::source_provenance(copied.path()).is_err());
        fs::remove_file(target).unwrap();
    }
    fs::remove_file(copied.path().join("openpilot/system/sentry.py")).unwrap();
    assert!(runtime::source_provenance(copied.path()).is_err());
}
#[test]
fn original_four_path_cli_preserves_raw_paths_and_exact_arity() {
    use std::os::unix::ffi::OsStringExt;
    let directory = tempfile::tempdir().unwrap();
    let raw = OsString::from_vec(b"daemon-\xff".to_vec());
    let args = [
        raw.clone(),
        "collector".into(),
        "binding".into(),
        "evidence".into(),
    ];
    let options = runtime::parse(&args, directory.path(), &environment()).unwrap();
    assert_eq!(options.daemon, directory.path().join(raw));
    assert_eq!(options.collector, directory.path().join("collector"));
    assert_eq!(options.params_binding, directory.path().join("binding"));
    assert_eq!(options.output, directory.path().join("evidence"));
    for count in 0..4 {
        assert!(runtime::parse(&args[..count], directory.path(), &environment()).is_err());
    }
    let mut extra = args.to_vec();
    extra.push("ignored?".into());
    assert!(runtime::parse(&extra, directory.path(), &environment()).is_err());
}
#[test]
fn original_real_five_second_scan_wire_disk_http_raw_names_and_four_startup_failures() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("runtime");
    let options = runtime::Options {
        daemon: artifact("TOMBSTONE_DAEMON_BINARY"),
        collector: artifact("TOMBSTONE_COLLECTOR_BINARY"),
        params_binding: artifact("TOMBSTONE_PARAMS_BINDING"),
        output: output.clone(),
        environment: environment(),
    };
    let report = runtime::run(&root(), &options, &artifact("TOMBSTONE_LIVE_WORKER")).unwrap();
    assert!(report.get("result").unwrap().text_eq("PASS"));
    assert_eq!(
        report
            .get("startup_failure_cases")
            .unwrap()
            .to_json()
            .unwrap(),
        "4"
    );
    let JsonView::Array(cases) = report.get("cases").unwrap().view() else {
        panic!("cases must be an array")
    };
    assert_eq!(cases.len(), 2);
    for (index, (case, enabled)) in cases.iter().zip([true, false]).enumerate() {
        assert!(case.get("result").unwrap().text_eq("PASS"));
        assert!(
            matches!(case.get("reporting_enabled").unwrap().view(),JsonView::Bool(value)if value==enabled)
        );
        for (key, count) in [
            ("log_messages", if enabled { 6 } else { 0 }),
            ("error_messages", if enabled { 3 } else { 0 }),
            ("disk_records", if enabled { 6 } else { 0 }),
            ("subsequent_cycle_duplicates", 0),
        ] {
            assert_eq!(
                case.get(key).unwrap().to_json().unwrap(),
                count.to_string(),
                "case{index} {key}"
            );
        }
        if enabled {
            let delay = case
                .get("new_crash_report_delay_seconds")
                .unwrap()
                .to_json()
                .unwrap()
                .parse::<f64>()
                .unwrap();
            assert!(
                (4.0..9.5).contains(&delay),
                "original strict scan timing changed: {delay}"
            );
        }
    }
    let event =
        J::parse(&fs::read_to_string(output.join("enabled/sdk-event.json")).unwrap()).unwrap();
    assert!(event
        .get("user")
        .unwrap()
        .get("id")
        .unwrap()
        .text_eq("fixture-device"));
    let tags = event.get("tags").unwrap();
    assert!(matches!(
        tags.get("dirty").unwrap().view(),
        JsonView::Bool(false)
    ));
    assert!(tags.get("device").unwrap().text_eq("tici"));
    let path = event.get("extra").unwrap().get("tombstone_fn").unwrap();
    let JsonView::Text(points) = path.view() else {
        panic!("path must retain raw filesystem text")
    };
    assert!(points.ends_with(
        &"new-"
            .chars()
            .map(u32::from)
            .chain([0xdcff, 45, 0xd55c, 45, 0xdce2, 0xdc82])
            .chain(".crash".chars().map(u32::from))
            .collect::<Vec<_>>()
    ));
    let failures =
        J::parse(&fs::read_to_string(output.join("startup-failures.json")).unwrap()).unwrap();
    let JsonView::Array(failures) = failures.view() else {
        panic!("failure array")
    };
    let JsonView::Array(original) = fixture()
        .get("runtime")
        .unwrap()
        .get("startup_failures")
        .unwrap()
        .view()
    else {
        panic!("original failure array")
    };
    assert_eq!(failures.len(), 4);
    for (actual, original) in failures.iter().zip(original) {
        assert_eq!(
            actual.get("exit_code").unwrap().to_json().unwrap(),
            original.get("exit_code").unwrap().to_json().unwrap()
        );
        assert_eq!(
            actual.get("stderr").unwrap().to_json().unwrap(),
            original.get("stderr").unwrap().to_json().unwrap()
        );
        assert_eq!(
            actual.get("stdout").is_some(),
            matches!(
                original.get("stdout_present").unwrap().view(),
                JsonView::Bool(true)
            )
        );
    }
    assert_eq!(
        fs::read_to_string(output.join("manifest.json")).unwrap(),
        runtime::format_report(&report).unwrap() + "\n"
    );
}

#[test]
fn source_preflight_failure_invalidates_prior_success_without_starting_worker() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("evidence");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("manifest.json"), b"{\"result\":\"PASS\"}\n").unwrap();
    let options = runtime::Options {
        daemon: "must-not-run".into(),
        collector: "missing".into(),
        params_binding: "missing".into(),
        output: output.clone(),
        environment: environment(),
    };
    assert!(runtime::run(root.path(), &options, Path::new("missing-worker")).is_err());
    assert!(!output.join("manifest.json").exists());
    assert_eq!(fs::read_dir(output).unwrap().count(), 0);
}

#[test]
fn binding_changed_after_daemon_ready_cannot_publish_a_success_for_mixed_artifacts() {
    use std::{
        io::Write,
        time::{Duration, Instant},
    };
    let temp = tempfile::tempdir().unwrap();
    let binding = temp.path().join("params.so");
    fs::copy(artifact("TOMBSTONE_PARAMS_BINDING"), &binding).unwrap();
    let output = temp.path().join("runtime");
    let observed = output.join("enabled/daemon.stderr");
    let mutation = binding.clone();
    let mutator = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if fs::read_to_string(&observed).is_ok_and(|text| text.contains("tombstoned: ready")) {
                let mut file = fs::OpenOptions::new().append(true).open(mutation).unwrap();
                file.write_all(b"changed provenance input after startup\n")
                    .unwrap();
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let options = runtime::Options {
        daemon: artifact("TOMBSTONE_DAEMON_BINARY"),
        collector: artifact("TOMBSTONE_COLLECTOR_BINARY"),
        params_binding: binding,
        output: output.clone(),
        environment: environment(),
    };
    let result = runtime::run(&root(), &options, &artifact("TOMBSTONE_LIVE_WORKER"));
    assert!(
        mutator.join().unwrap(),
        "daemon never reached acknowledged readiness for mutation"
    );
    assert!(
        result.is_err(),
        "checker accepted artifacts that changed during its live run"
    );
    assert!(
        !output.join("manifest.json").exists(),
        "mixed artifacts left a PASS manifest"
    );
}

#[test]
fn abrupt_worker_cleanup_kills_acknowledged_nested_session_preserves_foreign_child_and_ipc() {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::{
        os::fd::OwnedFd,
        process::{Child, Command},
        thread,
        time::{Duration, Instant},
    };
    struct Identity(OwnedFd);
    impl Drop for Identity {
        fn drop(&mut self) {
            let _ = pidfd_send_signal(&self.0, Signal::KILL);
        }
    }
    struct Foreign(Child);
    impl Drop for Foreign {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let ack = temp.path().join("live.pid");
    let release = temp.path().join("observed");
    let helper = root().join("rust/crates/dev-tools/tests/fixtures/tombstone-live/worker-abrupt");
    let mut env = environment();
    env.insert("TOMBSTONE_LIVE_TEST_ACK".into(), ack.as_os_str().to_owned());
    env.insert(
        "TOMBSTONE_LIVE_TEST_RELEASE".into(),
        release.as_os_str().to_owned(),
    );
    env.insert(
        "TOMBSTONE_LIVE_TEST_FIXTURE".into(),
        helper.as_os_str().to_owned(),
    );
    env.insert(
        "TOMBSTONE_LIVE_TEST_TOKEN".into(),
        temp.path().as_os_str().to_owned(),
    );
    let mut foreign = Foreign(
        Command::new("/bin/sleep")
            .arg("60")
            .env(
                "TOMBSTONE_REFERENCE_OWNER",
                temp.path().join("foreign-owner"),
            )
            .spawn()
            .unwrap(),
    );
    let options = runtime::Options {
        daemon: "/bin/true".into(),
        collector: "/bin/true".into(),
        params_binding: "/bin/true".into(),
        output: temp.path().join("evidence"),
        environment: env,
    };
    let (result, identity, prefix) = thread::scope(|scope| {
        let observer = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(pid) = fs::read_to_string(&ack)
                    .ok()
                    .and_then(|v| v.trim().parse::<i32>().ok())
                    .and_then(Pid::from_raw)
                {
                    if let Ok(identity) = pidfd_open(pid, PidfdFlags::empty()) {
                        if let Ok(env) = fs::read(format!("/proc/{}/environ", pid.as_raw_pid())) {
                            let entries = env.split(|b| *b == 0).collect::<Vec<_>>();
                            let mut token = b"TOMBSTONE_LIVE_TEST_TOKEN=".to_vec();
                            token.extend_from_slice(temp.path().as_os_str().as_encoded_bytes());
                            if entries.iter().any(|v| *v == token) {
                                let prefix = entries
                                    .iter()
                                    .find_map(|v| v.strip_prefix(b"OPENPILOT_PREFIX="))
                                    .map(|v| std::str::from_utf8(v).unwrap().to_owned())
                                    .unwrap();
                                fs::write(&release, b"observed").unwrap();
                                return (Identity(identity), prefix);
                            }
                        }
                    }
                }
                assert!(
                    Instant::now() < end,
                    "nested process did not acknowledge live identity"
                );
                thread::sleep(Duration::from_millis(1));
            }
        });
        let result = runtime::run(&root(), &options, &helper);
        let (identity, prefix) = observer.join().unwrap();
        (result, identity, prefix)
    });
    assert!(result.is_err(), "failed worker produced a success report");
    assert!(!options.output.join("manifest.json").exists());
    assert!(
        fs::read_to_string(options.output.join("enabled/worker.stderr"))
            .unwrap()
            .contains("intentional worker failure")
    );
    let end = Instant::now() + Duration::from_secs(1);
    loop {
        if poll(
            &mut [PollFd::new(&identity.0, PollFlags::IN)],
            Some(&Timespec::default()),
        )
        .unwrap()
            != 0
        {
            break;
        }
        assert!(
            Instant::now() < end,
            "nested worker session survived root failure"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        foreign.0.try_wait().unwrap().is_none(),
        "cleanup killed unrelated child"
    );
    assert!(
        !Path::new("/dev/shm")
            .join(format!("msgq_{prefix}"))
            .exists(),
        "owned IPC namespace survived failure"
    );
    assert!(!Path::new("/tmp")
        .join(format!("logmessage{prefix}"))
        .exists());
}

#[cfg(target_os = "linux")]
#[test]
fn sigint_at_final_provenance_write_cannot_leave_a_success_manifest() {
    use rustix::{
        fs::{mkfifoat, open, Mode, OFlags, CWD},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::{
        io::{ErrorKind, Read},
        os::fd::OwnedFd,
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    struct Checker {
        child: Child,
        identity: OwnedFd,
    }
    impl Drop for Checker {
        fn drop(&mut self) {
            // Keep the real outer owner alive for its normal group cleanup.
            let _ = pidfd_send_signal(&self.identity, Signal::INT);
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("runtime");
    let stderr = temp.path().join("checker.stderr");
    let child = Command::new(env!("CARGO_BIN_EXE_check-tombstoned-runtime"))
        .arg(artifact("TOMBSTONE_DAEMON_BINARY"))
        .arg(artifact("TOMBSTONE_COLLECTOR_BINARY"))
        .arg(artifact("TOMBSTONE_PARAMS_BINDING"))
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", root())
        .stdin(Stdio::null())
        .stdout(fs::File::create(temp.path().join("checker.stdout")).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker { child, identity };
    let deadline = Instant::now() + Duration::from_secs(90);
    let ready = output.join("enabled/daemon.stderr");
    loop {
        if fs::read_to_string(&ready).is_ok_and(|text| text.contains("tombstoned: ready")) {
            break;
        }
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "runtime exited before daemon acknowledgement: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "daemon did not acknowledge readiness"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let fifo = output.join("source-provenance.json");
    mkfifoat(CWD, &fifo, Mode::RUSR | Mode::WUSR).unwrap();
    let wchan = format!("/proc/{}/wchan", checker.child.id());
    // The main thread is blocked in open(our FIFO), after live checks and artifact
    // seals, before source-provenance/manifest publication. No guessed delay or
    // private production test hook chooses this final cancellation boundary.
    loop {
        if fs::read_to_string(&wchan).is_ok_and(|state| state.trim() == "wait_for_partner") {
            break;
        }
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "runtime exited before final writer boundary: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "checker never reached final provenance FIFO"
        );
        thread::sleep(Duration::from_millis(5));
    }
    pidfd_send_signal(&checker.identity, Signal::INT).unwrap();
    // The pending signal is delivered before the blocked open returns to user
    // code. A nonblocking reader releases it; all reads and exit waits are bounded.
    let mut reader = fs::File::from(
        open(
            &fifo,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .unwrap(),
    );
    let mut document = Vec::new();
    let finish = Instant::now() + Duration::from_secs(5);
    let status = loop {
        let mut bytes = [0u8; 8192];
        match reader.read(&mut bytes) {
            Ok(count) => document.extend_from_slice(&bytes[..count]),
            Err(error)
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
            Err(error) => panic!("provenance FIFO read failed: {error}"),
        }
        if let Some(status) = checker.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < finish,
            "interrupted final writer did not exit"
        );
        thread::sleep(Duration::from_millis(5));
    };
    assert!(
        !document.is_empty(),
        "the real final provenance writer was not exercised"
    );
    assert!(!status.success(), "interrupted checker exited successfully");
    let manifest = output.join("manifest.json");
    assert!(
        !manifest.exists(),
        "interrupted checker left a PASS manifest: {}",
        fs::read_to_string(&manifest).unwrap_or_default()
    );
}

#[path = "support/qa_endpoint_owner.rs"]
mod endpoint_owner;
#[test]
fn foreign_endpoint_after_live_collector_ack_survives_cancellation() {
    endpoint_owner::replacement_survives(
        &root(),
        Path::new(env!("CARGO_BIN_EXE_check-tombstoned-runtime")),
        &artifact("TOMBSTONE_COLLECTOR_BINARY"),
        Some(&artifact("TOMBSTONE_PARAMS_BINDING")),
    );
}

#[test]
fn four_partial_helpers_join_retired_runtime_history_without_optional_policy_inputs() {
    let current: serde_json::Value = serde_json::from_str(
        &runtime::source_provenance(&root())
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    let copied = tempfile::tempdir().unwrap();
    let retired = [
        "rust/tools/check_crash_sdk_transport.py",
        "rust/tools/check_tombstoned_reference.py",
        "rust/tools/tombstoned_reference.py",
        "rust/tools/check_version_reference.py",
    ];
    for name in current["source_sha256"].as_object().unwrap().keys() {
        if retired.contains(&name.as_str()) {
            continue;
        }
        let path = copied.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(root().join(name), path).unwrap();
    }
    let absent: serde_json::Value = serde_json::from_str(
        &runtime::source_provenance(copied.path())
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        absent["historical_source_sha256"]
            .as_object()
            .unwrap()
            .len(),
        6
    );
    for name in retired {
        assert!(absent["source_sha256"].get(name).is_none());
        assert_eq!(
            absent["historical_source_sha256"][name],
            current["source_sha256"]
                .get(name)
                .or_else(|| current["historical_source_sha256"].get(name))
                .unwrap()
                .clone()
        );
        let path = copied.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"changed retired source\n").unwrap();
        assert!(runtime::source_provenance(copied.path()).is_err());
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink("missing-retired", &path).unwrap();
        assert!(runtime::source_provenance(copied.path()).is_err());
        fs::remove_file(&path).unwrap();
    }
    fs::remove_file(copied.path().join("openpilot/system/sentry.py")).unwrap();
    assert!(runtime::source_provenance(copied.path()).is_err());
}
