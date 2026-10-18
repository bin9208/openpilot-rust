//! Independent original roster/CLI controls plus actual installed daemon gates.
#![cfg(feature = "managed-runtime-native")]
use openpilot_dev_tools::managed_runtime_check as check;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ffi::OsString, fs, path::PathBuf};
const ORIGINAL: &[u8] = include_bytes!("fixtures/managed-runtime/original.json");
fn original() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(ORIGINAL)),
        "71d405d8f0c3e4443188f1cf2ae4b41cb10e289937b52048026d46ec2647594c"
    );
    serde_json::from_slice(ORIGINAL).unwrap()
}
#[test]
fn all_seven_original_daemons_and_typed_failures_are_preserved() {
    let rows = original();
    assert_eq!(check::cases().len(), 7);
    for (actual, expected) in check::cases().iter().zip(rows["cases"].as_array().unwrap()) {
        assert_eq!(actual.binary, expected[0].as_str().unwrap());
        assert_eq!(actual.daemon, expected[1].as_str().unwrap());
        assert_eq!(actual.error_type, expected[2].as_str().unwrap());
    }
}
#[test]
fn original_cli_failures_help_prefixes_and_selection_keep_their_contract() {
    let rows = original();
    let environment = BTreeMap::new();
    let root = tempfile::tempdir().unwrap();
    for row in rows["cli"].as_array().unwrap() {
        let args = row["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| OsString::from(v.as_str().unwrap()))
            .collect::<Vec<_>>();
        let observed = check::parse(&args, root.path(), &environment);
        if row["name"] == "help" {
            assert!(matches!(observed.unwrap(), check::Command::Help));
        } else {
            let error = observed.unwrap_err().to_string();
            let expected = row["stderr"]
                .as_str()
                .unwrap()
                .lines()
                .last()
                .unwrap()
                .split_once(": error: ")
                .unwrap()
                .1;
            assert_eq!(error, expected, "original CLI {}", row["name"]);
        }
    }
    let parsed = check::parse(
        &[
            "--binar".into(),
            "relative-bin".into(),
            "--out".into(),
            "relative-out".into(),
            "--daemon".into(),
            "uploader".into(),
            "--omit-inherited-context".into(),
        ],
        root.path(),
        &environment,
    )
    .unwrap();
    let check::Command::Run(options) = parsed else {
        panic!("run expected")
    };
    assert_eq!(options.binaries, root.path().join("relative-bin"));
    assert_eq!(options.output, root.path().join("relative-out"));
    assert_eq!(options.daemon.as_deref(), Some("uploader"));
    assert!(!options.inherited_context);
    let clusters = include_bytes!("fixtures/managed-runtime/help-cluster-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(clusters)),
        "e1ba1541630b80605ebd8614047fb650e42f7f27c79bc8c4e9d51f721622f675"
    );
    let clusters: Value = serde_json::from_slice(clusters).unwrap();
    for row in clusters["cases"].as_array().unwrap() {
        let actual = check::parse(
            &[row["arg"].as_str().unwrap().into()],
            root.path(),
            &environment,
        );
        if row["exit"] == 0 {
            assert!(matches!(actual.unwrap(), check::Command::Help));
        } else {
            assert_eq!(
                actual.unwrap_err().to_string(),
                row["stderr"]
                    .as_str()
                    .unwrap()
                    .lines()
                    .last()
                    .unwrap()
                    .split_once(": error: ")
                    .unwrap()
                    .1
            );
        }
    }
}
#[test]
fn source_guard_allows_only_retired_absence_and_rejects_present_drift_or_aliases() {
    let root = tempfile::tempdir().unwrap();
    let baseline = check::sources(root.path()).unwrap();
    assert_eq!(baseline.len(), 1);
    let path = root.path().join("rust/tools/check_managed_runtime.py");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"changed checker").unwrap();
    assert!(check::sources(root.path()).is_err());
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(root.path().join("missing"), &path).unwrap();
    assert!(check::sources(root.path()).is_err());
    fs::remove_file(&path).unwrap();
    assert_eq!(check::sources(root.path()).unwrap(), baseline);
}
#[test]
fn all_seven_real_daemon_failures_and_standalone_help_work_with_both_context_modes() {
    let binaries = PathBuf::from(std::env::var_os("MANAGED_RUNTIME_BINARIES").expect("prerequisite: freshly built seven daemons and openpilot-logmessaged in MANAGED_RUNTIME_BINARIES"));
    let output = tempfile::tempdir().unwrap();
    let environment: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    for inherited in [true, false] {
        let options = check::Options {
            binaries: binaries.clone(),
            output: output
                .path()
                .join(if inherited { "inherited" } else { "omitted" }),
            daemon: None,
            inherited_context: inherited,
            environment: environment.clone(),
        };
        let receipt = match check::run(&options) {
            Ok(receipt) => receipt,
            Err(error) => {
                let retained = output.keep();
                panic!(
                    "actual daemon gate failed; evidence {}: {error}",
                    retained.display()
                )
            }
        };
        assert_eq!(receipt["result"], "PASS");
        let original_present = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../rust/tools/check_managed_runtime.py")
            .exists();
        let live = receipt["source_sha256"].as_object().unwrap();
        let historical = receipt["historical_source_sha256"].as_object().unwrap();
        assert_eq!(live.len(), usize::from(original_present));
        assert_eq!(historical.len(), usize::from(!original_present));
        assert_eq!(
            live.get("rust/tools/check_managed_runtime.py")
                .or_else(|| historical.get("rust/tools/check_managed_runtime.py"))
                .unwrap(),
            "a0cb645be563a6490c6df1c660d5fd764cb5ef5804158b82741e147fc87ac473"
        );
        let rows = receipt["scenarios"].as_array().unwrap();
        assert_eq!(rows.len(), 7);
        for (row, case) in rows.iter().zip(check::cases()) {
            assert_eq!(row["binary"], case.binary);
            assert_eq!(row["daemon"], case.daemon);
            assert_eq!(row["typed_failure"], case.error_type);
            assert_eq!(row["exit_code"], 1);
            assert_eq!(row["carrot_exception"], true);
            assert_eq!(row["standalone_help"], true);
            assert_eq!(row["inherited_context"], inherited);
            assert!(row["log_records"].as_u64().unwrap() >= 1);
        }
    }
}

#[test]
fn original_existing_output_and_child_spawn_failures_preserve_artifact_phases() {
    let captured = include_bytes!("fixtures/managed-runtime/failures-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(captured)),
        "1c5a8f9bef75f1b6cb00c9bf496ab2a067d02e51eb7a7415df67a0a87ddbfb53"
    );
    let original: Value = serde_json::from_slice(captured).unwrap();
    let binaries = PathBuf::from(
        std::env::var_os("MANAGED_RUNTIME_BINARIES")
            .expect("prerequisite: actual collector in MANAGED_RUNTIME_BINARIES"),
    );
    let root = tempfile::tempdir().unwrap();
    for row in original["cases"].as_array().unwrap() {
        let label = row["case"].as_str().unwrap();
        let bin = root.path().join(format!("{label}-bin"));
        fs::create_dir(&bin).unwrap();
        let output = root.path().join(label);
        if label == "existing-output" {
            fs::create_dir(&output).unwrap();
        }
        if label == "missing-daemon" {
            std::os::unix::fs::symlink(
                binaries.join("openpilot-logmessaged"),
                bin.join("openpilot-logmessaged"),
            )
            .unwrap();
        }
        let options = check::Options {
            binaries: bin,
            output: output.clone(),
            daemon: Some("uploader".into()),
            inherited_context: true,
            environment: std::env::vars_os().collect(),
        };
        let case = check::Case {
            binary: "missing-daemon",
            daemon: "uploader",
            error_type: "openpilot_uploader::",
        };
        assert!(
            check::run_case(&options, &case).is_err(),
            "original {label} must fail"
        );
        let expected = row["artifacts"].as_array().unwrap();
        for path in expected {
            assert!(
                output.join(path.as_str().unwrap()).exists(),
                "{label} lost original artifact {path}"
            );
        }
        for absent in ["stdout.txt", "stderr.txt", "result.json"] {
            assert!(
                !output.join(absent).exists(),
                "{label} advanced past original failure phase"
            );
        }
    }
}

#[test]
fn original_raw_argv_and_ambiguity_help_precedence_remain_lossless() {
    use std::os::unix::ffi::OsStringExt;
    let captured = include_bytes!("fixtures/managed-runtime/cli-edges-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(captured)),
        "57230e51bda95f9033ab5a4df613dfb1780add0231373c653c68283a5abeb078"
    );
    let original: Value = serde_json::from_slice(captured).unwrap();
    let negative_bytes = include_bytes!("fixtures/managed-runtime/negative-paths-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(negative_bytes)),
        "979541bf325a2d5883f92a2a01b3e9edb297976487a31e8f8323904c38a91b21"
    );
    let negative: Value = serde_json::from_slice(negative_bytes).unwrap();
    let root = tempfile::tempdir().unwrap();
    for row in original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(negative["cases"].as_array().unwrap())
    {
        let args = row["args_bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| {
                OsString::from_vec(
                    arg.as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u8)
                        .collect(),
                )
            })
            .collect::<Vec<_>>();
        let actual = check::parse(&args, root.path(), &BTreeMap::new());
        if row["exit"] == 0 {
            assert!(matches!(actual.unwrap(), check::Command::Help));
        } else {
            let bytes = row["stderr_bytes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8)
                .collect::<Vec<_>>();
            let text = std::str::from_utf8(&bytes).unwrap();
            let expected = text
                .lines()
                .last()
                .unwrap()
                .split_once(": error: ")
                .unwrap()
                .1;
            assert_eq!(
                actual.unwrap_err().to_string(),
                expected,
                "original {}",
                row["name"]
            );
        }
    }
    let raw = OsString::from_vec(b"bin-\xff".to_vec());
    let command = check::parse(
        &[
            "--binaries".into(),
            "ignored".into(),
            "--binaries".into(),
            raw.clone(),
            "--output".into(),
            "out".into(),
        ],
        root.path(),
        &BTreeMap::new(),
    )
    .unwrap();
    let check::Command::Run(options) = command else {
        panic!("run expected")
    };
    assert_eq!(options.binaries, root.path().join(raw));
    assert_eq!(options.daemon, None);
    assert!(options.inherited_context);
}

#[test]
fn failed_recheck_cannot_leave_a_stale_success_receipt() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("output");
    fs::create_dir(&output).unwrap();
    let receipt = output.join("result.json");
    fs::write(&receipt, b"{\"result\":\"PASS\"}\n").unwrap();
    let options = check::Options {
        binaries: root.path().join("missing-binaries"),
        output: output.clone(),
        daemon: Some("uploader".into()),
        inherited_context: true,
        environment: std::env::vars_os().collect(),
    };
    assert!(check::run(&options).is_err());
    assert!(
        !receipt.exists(),
        "stale PASS survived a failed current gate"
    );
    assert!(
        output.join("uploader/collector.log").exists(),
        "original collector spawn artifact phase lost"
    );
}

#[test]
fn actual_sigint_during_collector_startup_reaps_owned_child_and_removes_ipc() {
    use rustix::process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal};
    use std::{
        os::unix::fs::PermissionsExt,
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    struct Owned(Option<Child>);
    impl Drop for Owned {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    struct Worker(std::os::fd::OwnedFd);
    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = pidfd_send_signal(&self.0, Signal::KILL);
        }
    }
    let binary = std::env::var_os("MANAGED_RUNTIME_CHECKER")
        .expect("prerequisite: freshly built check-managed-runtime in MANAGED_RUNTIME_CHECKER");
    let root = tempfile::tempdir().unwrap();
    let bins = root.path().join("bin");
    fs::create_dir(&bins).unwrap();
    std::os::unix::fs::symlink(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/managed-runtime/fixture-launcher"),
        bins.join("openpilot-logmessaged"),
    )
    .unwrap();
    let body = root.path().join("body");
    fs::write(
        &body,
        b"printf '%s' \"$$\" > \"$MANAGED_RUNTIME_FIXTURE_PID\"\nexec /bin/sleep 30\n",
    )
    .unwrap();
    fs::set_permissions(&body, fs::Permissions::from_mode(0o400)).unwrap();
    let pidfile = root.path().join("pid");
    let output = root.path().join("output");
    let mut child = Owned(Some(
        Command::new(binary)
            .arg("--binaries")
            .arg(&bins)
            .arg("--output")
            .arg(&output)
            .args(["--daemon", "uploader"])
            .env("MANAGED_RUNTIME_FIXTURE_BODY", &body)
            .env("MANAGED_RUNTIME_FIXTURE_PID", &pidfile)
            .stdout(Stdio::from(
                fs::File::create(root.path().join("stdout")).unwrap(),
            ))
            .stderr(Stdio::from(
                fs::File::create(root.path().join("stderr")).unwrap(),
            ))
            .spawn()
            .unwrap(),
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        if let Ok(text) = fs::read_to_string(&pidfile) {
            if let Ok(pid) = text.parse::<i32>() {
                break Pid::from_raw(pid).unwrap();
            }
        }
        let status = child.0.as_mut().unwrap().try_wait().unwrap();
        if let Some(status) = status {
            child.0 = None;
            panic!(
                "checker exited before collector fixture: {status}; {}",
                fs::read_to_string(root.path().join("stderr")).unwrap()
            );
        }
        assert!(
            Instant::now() < deadline,
            "sleeping collector fixture absent"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    let _worker = Worker(pidfd_open(pid, PidfdFlags::empty()).unwrap());
    let raw = fs::read(format!("/proc/{}/environ", pid.as_raw_pid())).unwrap();
    let prefix = raw
        .split(|b| *b == 0)
        .find_map(|v| v.strip_prefix(b"OPENPILOT_PREFIX="))
        .unwrap();
    let prefix = std::str::from_utf8(prefix).unwrap();
    let shm = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
    let endpoint = PathBuf::from(format!("/tmp/logmessage{prefix}"));
    let checker = Pid::from_raw(child.0.as_ref().unwrap().id() as i32).unwrap();
    rustix::process::kill_process(checker, Signal::INT).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.as_mut().unwrap().try_wait().unwrap() {
            child.0 = None;
            assert_eq!(
                status.code(),
                Some(130),
                "native SIGINT shell status must match original -2"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "SIGINT cleanup exceeded actual original bound"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !PathBuf::from(format!("/proc/{}", pid.as_raw_pid())).exists(),
        "collector not reaped"
    );
    assert!(!shm.exists(), "owned namespace leaked");
    assert!(!endpoint.exists(), "owned endpoint leaked");
    assert!(
        !output.join("result.json").exists(),
        "cancelled gate published PASS"
    );
}

#[test]
fn unknown_programmatic_selection_cannot_publish_zero_case_success() {
    let root = tempfile::tempdir().unwrap();
    let bins = root.path().join("bin");
    fs::create_dir(&bins).unwrap();
    // No child should execute. A valid existing ELF used only as a hash makes
    // the previous empty-loop PASS reachable without an invented daemon reply.
    std::os::unix::fs::symlink(
        std::env::current_exe().unwrap(),
        bins.join("openpilot-logmessaged"),
    )
    .unwrap();
    let output = root.path().join("output");
    fs::create_dir(&output).unwrap();
    let options = check::Options {
        binaries: bins,
        output: output.clone(),
        daemon: Some("not-a-daemon".into()),
        inherited_context: true,
        environment: std::env::vars_os().collect(),
    };
    assert!(
        check::run(&options).is_err(),
        "zero selected daemons yielded PASS"
    );
    assert!(!output.join("result.json").exists());
}

#[test]
fn actual_final_result_write_sigint_cannot_publish_success() {
    use rustix::{
        fs::{mkfifoat, open, Mode, OFlags, CWD},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::{
        io::{Read, Write},
        os::{fd::OwnedFd, unix::fs::PermissionsExt},
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    struct Checker {
        child: Child,
        identity: OwnedFd,
    }
    impl Drop for Checker {
        fn drop(&mut self) {
            let _ = pidfd_send_signal(&self.identity, Signal::INT);
            let end = Instant::now() + Duration::from_secs(2);
            while Instant::now() < end {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    let binary = std::env::var_os("MANAGED_RUNTIME_CHECKER").expect("mandatory fresh checker");
    let actual = PathBuf::from(
        std::env::var_os("MANAGED_RUNTIME_BINARIES")
            .expect("mandatory fresh actual daemon directory"),
    );
    let root = tempfile::tempdir().unwrap();
    let bins = root.path().join("bin");
    fs::create_dir(&bins).unwrap();
    std::os::unix::fs::symlink(
        actual.join("openpilot-logmessaged"),
        bins.join("openpilot-logmessaged"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/managed-runtime/fixture-launcher"),
        bins.join("openpilot-uploader"),
    )
    .unwrap();
    let ready = root.path().join("ready");
    let release = root.path().join("release");
    mkfifoat(CWD, &release, Mode::RUSR | Mode::WUSR).unwrap();
    let body = root.path().join("body");
    fs::write(&body,b"\"$MANAGED_ACTUAL_DAEMON\" \"$@\"\nstatus=$?\nif [ \"$1\" = --invalid-managed-fixture ]; then\n printf 1 > \"$MANAGED_DAEMON_READY\"\n read -r answer < \"$MANAGED_DAEMON_RELEASE\"\nfi\nexit \"$status\"\n").unwrap();
    fs::set_permissions(&body, fs::Permissions::from_mode(0o400)).unwrap();
    let output = root.path().join("output");
    let stderr = root.path().join("stderr");
    let child = Command::new(binary)
        .arg("--binaries")
        .arg(&bins)
        .arg("--output")
        .arg(&output)
        .args(["--daemon", "uploader"])
        .env("MANAGED_RUNTIME_FIXTURE_BODY", &body)
        .env("MANAGED_ACTUAL_DAEMON", actual.join("openpilot-uploader"))
        .env("MANAGED_DAEMON_READY", &ready)
        .env("MANAGED_DAEMON_RELEASE", &release)
        .stdout(Stdio::from(
            fs::File::create(root.path().join("stdout")).unwrap(),
        ))
        .stderr(Stdio::from(fs::File::create(&stderr).unwrap()))
        .spawn()
        .unwrap();
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker { child, identity };
    let end = Instant::now() + Duration::from_secs(30);
    while !ready.exists() {
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "checker exited before actual daemon gate: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        assert!(Instant::now() < end, "actual uploader gate absent");
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = output.join("result.json");
    mkfifoat(CWD, &result, Mode::RUSR | Mode::WUSR).unwrap();
    let mut writer = loop {
        match open(
            &release,
            OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => break fs::File::from(fd),
            Err(rustix::io::Errno::NXIO) => {
                assert!(Instant::now() < end);
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("release open: {e}"),
        }
    };
    writer.write_all(b"continue\n").unwrap();
    drop(writer);
    let wchan = format!("/proc/{}/wchan", checker.child.id());
    while !fs::read_to_string(&wchan).is_ok_and(|s| s.trim() == "wait_for_partner") {
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "checker exited before final FIFO: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        assert!(Instant::now() < end, "actual final writer boundary absent");
        std::thread::sleep(Duration::from_millis(5));
    }
    pidfd_send_signal(&checker.identity, Signal::INT).unwrap();
    let mut reader = fs::File::from(
        open(
            &result,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .unwrap(),
    );
    let mut bytes = Vec::new();
    let end = Instant::now() + Duration::from_secs(5);
    let status = loop {
        let mut buffer = [0; 8192];
        match reader.read(&mut buffer) {
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) => panic!("result FIFO: {e}"),
        };
        if let Some(status) = checker.child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < end, "interrupted writer stuck");
        std::thread::sleep(Duration::from_millis(5));
    };
    let report: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(report["result"], "PASS", "real final write not exercised");
    assert_eq!(report["scenarios"].as_array().unwrap().len(), 1);
    assert_eq!(status.code(), Some(130));
    assert!(
        !result.exists(),
        "interrupted final write left success receipt"
    );
}

fn fifo_executable_failure(collector: bool) {
    use rustix::{
        fs::{mkfifoat, Mode, CWD},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::{
        os::fd::OwnedFd,
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    struct Checker {
        child: Child,
        identity: OwnedFd,
    }
    impl Drop for Checker {
        fn drop(&mut self) {
            let _ = pidfd_send_signal(&self.identity, Signal::KILL);
            let _ = self.child.wait();
        }
    }
    let actual = PathBuf::from(
        std::env::var_os("MANAGED_RUNTIME_BINARIES").expect("fresh actual daemon prerequisite"),
    );
    let checker = std::env::var_os("MANAGED_RUNTIME_CHECKER").expect("fresh checker prerequisite");
    let root = tempfile::tempdir().unwrap();
    let bins = root.path().join("bin");
    fs::create_dir(&bins).unwrap();
    for name in ["openpilot-uploader", "openpilot-logmessaged"] {
        if (name == "openpilot-logmessaged") == collector {
            // No writer is ever opened: execve must reject this input, rather
            // than a provenance read waiting indefinitely for a FIFO partner.
            mkfifoat(CWD, bins.join(name), Mode::RUSR | Mode::WUSR | Mode::XUSR).unwrap();
        } else {
            std::os::unix::fs::symlink(actual.join(name), bins.join(name)).unwrap();
        }
    }
    let output = root.path().join("output");
    let stderr = root.path().join("stderr");
    let child = Command::new(checker)
        .arg("--binaries")
        .arg(&bins)
        .arg("--output")
        .arg(&output)
        .args(["--daemon", "uploader"])
        .stdout(Stdio::from(
            fs::File::create(root.path().join("stdout")).unwrap(),
        ))
        .stderr(Stdio::from(fs::File::create(&stderr).unwrap()))
        .spawn()
        .unwrap();
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut owned = Checker { child, identity };
    let end = Instant::now() + Duration::from_secs(5);
    let mut blocked_hash = false;
    let status = loop {
        if let Some(status) = owned.child.try_wait().unwrap() {
            break Some(status);
        }
        if fs::read_to_string(format!("/proc/{}/wchan", owned.child.id()))
            .is_ok_and(|text| text.trim() == "wait_for_partner")
        {
            blocked_hash = true;
            break None;
        }
        if Instant::now() >= end {
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let status = if let Some(status) = status {
        Some(status)
    } else {
        // Also preserve the demonstrated cancellation failure: if prehash
        // blocks, SIGINT alone must terminate; an external writer is not a fix.
        pidfd_send_signal(&owned.identity, Signal::INT).unwrap();
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = owned.child.try_wait().unwrap() {
                break Some(status);
            }
            if Instant::now() >= end {
                break None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    if status.is_none() {
        // Failure-only rescue for testing the old blocking implementation:
        // allow its Rust owners to unwind instead of leaking IPC via SIGKILL.
        // This never changes the failed bounded-cancellation result above.
        let fifo = bins.join(if collector {
            "openpilot-logmessaged"
        } else {
            "openpilot-uploader"
        });
        let end = Instant::now() + Duration::from_secs(5);
        while owned.child.try_wait().unwrap().is_none() && Instant::now() < end {
            if let Ok(fd) = rustix::fs::open(
                &fifo,
                rustix::fs::OFlags::WRONLY
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                drop(fd);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let error = fs::read_to_string(&stderr).unwrap();
    assert!(
        status.is_some(),
        "FIFO {} input ignored SIGINT while hashing; stderr={error}",
        if collector { "collector" } else { "daemon" }
    );
    assert!(
        !blocked_hash,
        "FIFO executable was read before its spawn failure"
    );
    assert_eq!(
        status.unwrap().code(),
        Some(1),
        "original exec failure: {error}"
    );
    assert!(
        error.contains("Permission denied"),
        "spawn phase lost: {error}"
    );
    let case = output.join("uploader");
    assert!(
        case.join("collector.log").is_file(),
        "original log-open phase lost"
    );
    assert_eq!(case.join("logs").is_dir(), !collector);
    for file in ["stdout.txt", "stderr.txt", "result.json"] {
        assert!(
            !case.join(file).exists(),
            "advanced beyond failed exec: {file}"
        );
    }
    assert!(
        !output.join("result.json").exists(),
        "failed FIFO input published PASS"
    );
}

#[test]
fn fifo_daemon_is_rejected_at_spawn_without_an_uncancellable_hash() {
    fifo_executable_failure(false);
}

#[test]
fn fifo_collector_is_rejected_at_spawn_without_an_uncancellable_hash() {
    fifo_executable_failure(true);
}
