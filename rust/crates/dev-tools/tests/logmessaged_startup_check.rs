//! Source-derived contract tests. Runtime execution belongs to GitHub Actions.
#![cfg(all(target_os = "linux", feature = "logmessaged-startup-native"))]
use openpilot_dev_tools::logmessaged_startup_check::{self as checker, Options};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn executable() -> PathBuf {
    Path::new(env!("CARGO_BIN_EXE_check-logmessaged-startup")).into()
}
fn collector() -> PathBuf {
    std::env::var_os("LOGMESSAGED_COLLECTOR_BINARY")
        .expect("fresh production collector required")
        .into()
}
fn options(path: &Path) -> Options {
    Options {
        repository: repository(),
        collector: collector(),
        output: path.into(),
    }
}
fn copied_inputs(copy: &Path) {
    let pins: std::collections::BTreeMap<String, String> = serde_json::from_str(include_str!(
        "fixtures/logmessaged-startup/source-pins.json"
    ))
    .unwrap();
    let authored = [
        "rust/crates/dev-tools/src/logmessaged_startup_check.rs",
        "rust/crates/dev-tools/src/logmessaged_startup_check/worker.rs",
        "rust/crates/dev-tools/src/bin/check_logmessaged_startup.rs",
        "rust/crates/dev-tools/src/managed_entry_collector_source.rs",
        "rust/crates/dev-tools/src/stats_daemon_check/process.rs",
        "rust/crates/dev-tools/src/stats_daemon_check/identity.rs",
        "rust/crates/dev-tools/src/qa_endpoint.rs",
        "rust/crates/dev-tools/src/qa_process.rs",
        "rust/crates/dev-tools/src/argparse_scan.rs",
        "rust/crates/dev-tools/src/python_fs.rs",
        "rust/crates/dev-tools/src/path_resolve.rs",
        "rust/crates/dev-tools/tests/fixtures/logmessaged-startup/source-pins.json",
        "rust/crates/dev-tools/tests/fixtures/logmessaged-startup/original-observation.json",
    ];
    for name in pins.keys().map(String::as_str).chain(authored) {
        if name == "rust/tools/check_logmessaged_startup.py" {
            continue;
        }
        let target = copy.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(repository().join(name), target).unwrap();
    }
}
#[test]
fn independently_read_original_three_case_contract_rejects_missing_extra_or_wrong_records() {
    assert_eq!(
        checker::CASES,
        ["legacy-source", "ready-source", "ready-native"]
    );
    let original = json!({"msg":"first-error-after-start","levelnum":40});
    assert_eq!(
        serde_json::from_str::<Value>(checker::MARKER).unwrap(),
        original
    );
    for (name, errors) in [
        ("legacy-source", 0),
        ("ready-source", 1),
        ("ready-native", 1),
    ] {
        let log = json!([checker::MARKER]);
        let error = if errors == 0 { json!([]) } else { log.clone() };
        let observed = json!({"logMessage":log,"errorLogMessage":error});
        let disk =
            vec![json!({"msg$s":"first-error-after-start","levelnum":40,"id":"independent-id"})];
        let counts = checker::validate_observed(name, &observed, &disk).unwrap();
        assert_eq!(counts, json!({"logMessage":1,"errorLogMessage":errors}));
        for wrong in [
            json!({"logMessage":[],"errorLogMessage":error}),
            json!({"logMessage":[checker::MARKER,checker::MARKER],"errorLogMessage":error}),
            json!({"logMessage":["different"],"errorLogMessage":error}),
            json!({"logMessage":log,"errorLogMessage":if errors==0 {json!([checker::MARKER])}else{json!([])}}),
        ] {
            assert!(
                checker::validate_observed(name, &wrong, &disk).is_err(),
                "{name}: {wrong}"
            );
        }
        for wrong in [
            vec![],
            vec![disk[0].clone(), disk[0].clone()],
            vec![json!({"msg":"first-error-after-start"})],
        ] {
            assert!(checker::validate_observed(name, &observed, &wrong).is_err());
        }
    }
}
#[test]
fn original_cli_grammar_keeps_help_required_flags_abbreviations_and_raw_paths() {
    use std::os::unix::ffi::OsStringExt;
    assert!(checker::parse(&["--help".into()]).unwrap().is_none());
    for args in [
        vec![],
        vec!["--collector", "x"],
        vec!["--output", "y"],
        vec!["--collector", "x", "--output", "y", "extra"],
        vec!["--unknown"],
        vec!["--collector"],
    ] {
        let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
        assert!(checker::parse(&args).is_err());
    }
    let raw = OsString::from_vec(b"private-\xff".to_vec());
    let args = vec![
        "--out".into(),
        raw.clone(),
        "--coll".into(),
        "collector".into(),
    ];
    let parsed = checker::parse(&args).unwrap().unwrap();
    assert!(parsed.output.ends_with(Path::new(&raw)));
}
#[test]
fn actual_cli_help_and_usage_do_not_open_or_launch_a_collector() {
    let help = Command::new(executable()).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(help.stderr.is_empty());
    for args in [
        vec![],
        vec!["--collector", "missing"],
        vec!["--output"],
        vec!["--runner", "/bin/false"],
    ] {
        let result = Command::new(executable()).args(args).output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(!result.stderr.is_empty());
    }
}
#[test]
fn all_three_real_collector_observations_and_disk_records_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(&temp.path().join("long-output-".repeat(15)));
    assert!(options.output.as_os_str().as_encoded_bytes().len() > 108);
    let result = checker::run(&options, &executable()).unwrap();
    let bytes = result.to_json().unwrap();
    let report: Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["compiler"], json!([]));
    assert!(bytes.find("\"legacy-source\"").unwrap() < bytes.find("\"ready-source\"").unwrap());
    assert!(bytes.find("\"ready-source\"").unwrap() < bytes.find("\"ready-native\"").unwrap());
    for (name, errors) in [
        ("legacy-source", 0),
        ("ready-source", 1),
        ("ready-native", 1),
    ] {
        assert_eq!(
            report["cases"][name],
            json!({"logMessage":1,"errorLogMessage":errors})
        );
        let raw: Value = serde_json::from_slice(
            &fs::read(options.output.join(name).join("observed.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(raw["logMessage"], json!([checker::MARKER]));
        assert_eq!(raw["errorLogMessage"].as_array().unwrap().len(), errors);
        assert!(!options.output.join(name).join("publisher.gate").exists());
        assert!(options.output.join(name).join("logMessage.bin").is_file());
    }
    assert_eq!(
        fs::read(options.output.join("report.json")).unwrap(),
        bytes.as_bytes()
    );
}
#[test]
fn existing_output_receipt_is_not_overwritten_or_removed() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("owned-by-other");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("report.json"), b"foreign receipt").unwrap();
    assert!(checker::run(&options(&output), &executable()).is_err());
    assert_eq!(
        fs::read(output.join("report.json")).unwrap(),
        b"foreign receipt"
    );
}
#[test]
fn changed_or_nonregular_historical_checker_fails_before_worker_or_success() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    let temp = tempfile::tempdir().unwrap();
    let copy = temp.path().join("repository");
    copied_inputs(&copy);
    let source = copy.join("rust/tools/check_logmessaged_startup.py");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    for kind in ["changed", "fifo", "dangling", "symlink", "directory"] {
        if source.exists() || fs::symlink_metadata(&source).is_ok() {
            if source.is_dir() {
                fs::remove_dir(&source).unwrap();
            } else {
                fs::remove_file(&source).unwrap();
            }
        }
        match kind {
            "fifo" => rustix::fs::mkfifoat(
                rustix::fs::CWD,
                &source,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            )
            .unwrap(),
            "dangling" => std::os::unix::fs::symlink("missing", &source).unwrap(),
            "symlink" => {
                std::os::unix::fs::symlink(copy.join("rust/tools/logmessaged_native.py"), &source)
                    .unwrap()
            }
            "directory" => fs::create_dir(&source).unwrap(),
            _ => fs::write(&source, b"changed original").unwrap(),
        }
        let output = temp.path().join(kind);
        let mut spec = CommandSpec::new_os(
            vec![
                executable().into_os_string(),
                "--collector".into(),
                collector().into_os_string(),
                "--output".into(),
                output.clone().into_os_string(),
            ],
            Duration::from_secs(5),
        );
        spec.set_os_env(std::collections::BTreeMap::from([(
            "OPENPILOT_RUST_ROOT".into(),
            copy.clone().into_os_string(),
        )]));
        let observed = NativeRunner.run(&spec).unwrap();
        assert_eq!(observed.code, 1, "{kind}");
        assert!(!output.join("legacy-source").exists());
        assert!(!output.join("report.json").exists());
    }
}
#[test]
fn only_historical_checker_has_an_absence_exception() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    let temp = tempfile::tempdir().unwrap();
    let copy = temp.path().join("repository");
    copied_inputs(&copy);
    fs::remove_file(copy.join("rust/tools/logmessaged_native.py")).unwrap();
    let output = temp.path().join("missing-shared-source");
    let mut spec = CommandSpec::new_os(
        vec![
            executable().into_os_string(),
            "--collector".into(),
            collector().into_os_string(),
            "--output".into(),
            output.clone().into_os_string(),
        ],
        Duration::from_secs(5),
    );
    spec.set_os_env(std::collections::BTreeMap::from([(
        "OPENPILOT_RUST_ROOT".into(),
        copy.into_os_string(),
    )]));
    let observed = NativeRunner.run(&spec).unwrap();
    assert_eq!(observed.code, 1);
    assert!(!output.join("legacy-source").exists());
    assert!(!output.join("report.json").exists());
}
#[test]
fn only_exact_historical_checker_absence_is_accepted_and_reappearance_cannot_publish() {
    struct Owned(Option<std::process::Child>);
    impl Drop for Owned {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let copy = temp.path().join("repository");
    copied_inputs(&copy);
    let absent = copy.join("rust/tools/check_logmessaged_startup.py");
    assert!(!absent.exists());
    let output = temp.path().join("absent");
    let report = checker::run(
        &Options {
            repository: copy.clone(),
            collector: collector(),
            output: output.clone(),
        },
        &executable(),
    )
    .unwrap();
    let report: Value = serde_json::from_str(&report.to_json().unwrap()).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(
        report["historical_source_sha256"]["rust/tools/check_logmessaged_startup.py"],
        "46cae42fe4749a48bb9f0acc81b92db974288633d28100b0a023b3d002b9b022"
    );
    assert_eq!(
        report["reference_observation_sha256"],
        "6b3a28eb057aac4dc3bdf3088015711e7bf684acda5e8c40cb2af2d52b92ef15"
    );
    assert!(report["input_sha256"]
        .get("rust/tools/check_logmessaged_startup.py")
        .is_none());
    let output = temp.path().join("reappearance");
    let child = Command::new(executable())
        .args([
            "--collector".into(),
            collector().into_os_string(),
            "--output".into(),
            output.clone().into_os_string(),
        ])
        .env("OPENPILOT_RUST_ROOT", &copy)
        .stdout(Stdio::null())
        .stderr(fs::File::create(temp.path().join("reappearance.stderr")).unwrap())
        .spawn()
        .unwrap();
    let mut owner = Owned(Some(child));
    let deadline = Instant::now() + Duration::from_secs(20);
    while !output.join("legacy-source/process-ready.json").is_file() {
        assert!(
            owner.0.as_mut().unwrap().try_wait().unwrap().is_none(),
            "checker exited before first worker ACK"
        );
        assert!(Instant::now() < deadline, "first worker ACK absent");
        std::thread::sleep(Duration::from_millis(5));
    }
    fs::create_dir_all(absent.parent().unwrap()).unwrap();
    fs::write(&absent, b"reappeared during native comparison").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = owner.0.as_mut().unwrap().try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "checker did not reject reappearance"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    let _ = owner.0.take();
    assert!(!status.success());
    assert!(!output.join("report.json").exists());
}
#[test]
fn leader_only_cancellation_stops_owned_children_and_preserves_foreign_replacement() {
    for replacement in [false, true] {
        cancel(replacement);
    }
}
fn cancel(replacement: bool) {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::os::fd::OwnedFd;
    struct Owner {
        child: std::process::Child,
        id: OwnedFd,
        descendants: Vec<OwnedFd>,
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = pidfd_send_signal(&self.id, Signal::KILL);
            for id in &self.descendants {
                let _ = pidfd_send_signal(id, Signal::KILL);
            }
            let _ = self.child.wait();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("proof");
    let token = format!(
        "LOG_STARTUP_TOKEN={}",
        temp.path().file_name().unwrap().to_string_lossy()
    );
    let child = Command::new(executable())
        .args([
            OsString::from("--collector"),
            collector().into_os_string(),
            "--output".into(),
            output.clone().into_os_string(),
        ])
        .env("OPENPILOT_RUST_ROOT", repository())
        .env(
            "LOG_STARTUP_TOKEN",
            token.strip_prefix("LOG_STARTUP_TOKEN=").unwrap(),
        )
        .stdout(Stdio::null())
        .stderr(fs::File::create(temp.path().join("stderr")).unwrap())
        .spawn()
        .unwrap();
    let id = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut owner = Owner {
        child,
        id,
        descendants: vec![],
    };
    let end = Instant::now() + Duration::from_secs(30);
    let ack = loop {
        if let Ok(bytes) = fs::read(output.join("legacy-source/process-ready.json")) {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                break value;
            }
        }
        assert!(
            owner.child.try_wait().unwrap().is_none(),
            "{}",
            fs::read_to_string(temp.path().join("stderr")).unwrap()
        );
        assert!(
            Instant::now() < end,
            "collector ownership ACK was not reached"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    for name in ["worker", "collector"] {
        let pid = Pid::from_raw(i32::try_from(ack[name].as_i64().unwrap()).unwrap()).unwrap();
        let id = pidfd_open(pid, PidfdFlags::empty()).unwrap();
        loop {
            let environment =
                fs::read(format!("/proc/{}/environ", pid.as_raw_pid())).unwrap_or_default();
            if environment
                .split(|b| *b == 0)
                .any(|v| v == token.as_bytes())
            {
                break;
            }
            assert_eq!(
                poll(
                    &mut [PollFd::new(&id, PollFlags::IN)],
                    Some(&Timespec::default())
                )
                .unwrap(),
                0
            );
            assert!(Instant::now() < end, "private environment ACK absent");
            std::thread::sleep(Duration::from_millis(5));
        }
        owner.descendants.push(id);
    }
    let prefix = ack["prefix"].as_str().unwrap();
    let endpoint = PathBuf::from(format!("/tmp/logmessage{prefix}"));
    let foreign = if replacement {
        fs::remove_file(&endpoint).unwrap();
        Some(std::os::unix::net::UnixListener::bind(&endpoint).unwrap())
    } else {
        None
    };
    pidfd_send_signal(&owner.id, Signal::INT).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        let code = owner.child.try_wait().unwrap();
        let gone = owner.descendants.iter().all(|id| {
            poll(
                &mut [PollFd::new(id, PollFlags::IN)],
                Some(&Timespec::default()),
            )
            .unwrap()
                == 1
        });
        if let Some(code) = code {
            assert!(!code.success());
            if gone {
                break;
            }
        }
        assert!(
            Instant::now() < end,
            "owned collector survived cancellation"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!output.join("report.json").exists());
    assert!(!PathBuf::from(format!("/dev/shm/msgq_{prefix}")).exists());
    if foreign.is_some() {
        assert!(endpoint.exists(), "foreign replacement was deleted");
        drop(foreign);
        fs::remove_file(endpoint).unwrap();
    } else {
        assert!(!endpoint.exists());
    }
}

#[test]
fn final_stdout_sigint_after_all_three_results_cannot_publish_success() {
    use rustix::{
        fs::{fcntl_getfl, fcntl_setfl, OFlags},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::io::{ErrorKind, Read, Write};
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("proof");
    let (mut reader, mut writer) = std::io::pipe().unwrap();
    let flags = fcntl_getfl(&writer).unwrap();
    fcntl_setfl(&writer, flags | OFlags::NONBLOCK).unwrap();
    loop {
        match writer.write(&[b'x'; 4096]) {
            Ok(n) => assert!(n > 0),
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == ErrorKind::Interrupted => (),
            Err(e) => panic!("{e}"),
        }
    }
    fcntl_setfl(&writer, flags).unwrap();
    let flags = fcntl_getfl(&reader).unwrap();
    fcntl_setfl(&reader, flags | OFlags::NONBLOCK).unwrap();
    struct Owner {
        child: std::process::Child,
        id: std::os::fd::OwnedFd,
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = pidfd_send_signal(&self.id, Signal::KILL);
            let _ = self.child.wait();
        }
    }
    let child = Command::new(executable())
        .args([
            "--collector".into(),
            collector().into_os_string(),
            "--output".into(),
            output.clone().into_os_string(),
        ])
        .env("OPENPILOT_RUST_ROOT", repository())
        .stdout(Stdio::from(writer))
        .stderr(fs::File::create(temp.path().join("stderr")).unwrap())
        .spawn()
        .unwrap();
    let id = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut owner = Owner { child, id };
    let end = Instant::now() + Duration::from_secs(90);
    loop {
        let finished = ["legacy-source", "ready-source", "ready-native"]
            .iter()
            .all(|name| output.join(name).join("collector/exit.json").is_file());
        let state =
            fs::read_to_string(format!("/proc/{}/wchan", owner.child.id())).unwrap_or_default();
        if finished && state.contains("pipe_write") {
            break;
        }
        assert!(
            owner.child.try_wait().unwrap().is_none(),
            "{}",
            fs::read_to_string(temp.path().join("stderr")).unwrap()
        );
        assert!(
            Instant::now() < end,
            "three results/final stdout boundary absent: {state}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!output.join("report.json").exists());
    pidfd_send_signal(&owner.id, Signal::INT).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let mut bytes = [0; 8192];
        loop {
            match reader.read(&mut bytes) {
                Ok(0) => break,
                Ok(_) => (),
                Err(e) if e.kind() == ErrorKind::Interrupted => (),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("{e}"),
            }
        }
        if let Some(status) = owner.child.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        assert!(
            Instant::now() < end,
            "checker survived final stdout cancellation"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!output.join("report.json").exists());
}
