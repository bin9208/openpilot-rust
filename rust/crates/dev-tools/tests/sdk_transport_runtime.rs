//! Real native Sentry HTTP transport, independent source policy, and SDK capture fields.
#![cfg(all(feature = "sdk-transport-native", feature = "tombstone-tools-native"))]
use openpilot_dev_tools::{sdk_transport_check, tombstone_reference_check::Options};
use openpilot_log_format::JsonValue as J;
use std::{fs, path::Path};
#[test]
fn fourteen_real_sdk_envelopes_and_native_io_exception_keep_original_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let required = |key| {
        fs::canonicalize(
            std::env::var_os(key)
                .unwrap_or_else(|| panic!("{key} must name verified current-source artifact")),
        )
        .unwrap()
    };
    let temp = tempfile::tempdir().unwrap();
    let options = Options {
        binary: required("TOMBSTONE_PROBE_BINARY"),
        params_binding: required("TOMBSTONE_PARAMS_BINDING"),
        runner: vec![],
        output: temp.path().join("sdk"),
        environment: std::env::vars_os().collect(),
    };
    let report = sdk_transport_check::run(&root, &options).unwrap();
    assert!(report.get("result").unwrap().text_eq("PASS"));
    assert_eq!(report.get("events").unwrap().to_json().unwrap(), "14");
    assert_eq!(
        report
            .get("native_io_exception_events")
            .unwrap()
            .to_json()
            .unwrap(),
        "1"
    );
    let disk =
        J::parse(&fs::read_to_string(options.output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(disk.to_json().unwrap(), report.to_json().unwrap());
    for name in (0..13).map(|index| format!("envelope-{index}.txt")).chain([
        "envelope-dirty.txt".into(),
        "envelope-native-error.txt".into(),
    ]) {
        assert!(!fs::read(options.output.join(name)).unwrap().is_empty());
    }
    let details =
        J::parse(&fs::read_to_string(options.output.join("native-error.json")).unwrap()).unwrap();
    assert!(details.get("event").is_some());
    assert!(options.output.join("policy/comparisons.jsonl").is_file());
}

#[test]
fn invalid_source_removes_prior_success_before_any_transport_work() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("sdk");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("manifest.json"), "{\"result\":\"PASS\"}\n").unwrap();
    let options = Options {
        binary: temp.path().join("missing-probe"),
        params_binding: temp.path().join("missing-binding"),
        runner: vec![],
        output,
        environment: std::env::vars_os().collect(),
    };
    assert!(sdk_transport_check::run(temp.path(), &options).is_err());
    assert!(!options.output.join("manifest.json").exists());
    assert!(!options.output.join("policy").exists());
}

#[test]
fn cli_failure_does_not_publish_or_retain_a_success_receipt() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let cli = env!("CARGO_BIN_EXE_check-crash-sdk-transport");
    for args in [vec![], vec!["--source"], vec!["missing", "binding"]] {
        let result = Command::new(cli)
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    let output = temp.path().join("result");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("manifest.json"), "{\"result\":\"PASS\"}\n").unwrap();
    let result = Command::new(cli)
        .args(["missing", "binding", "result"])
        .env("OPENPILOT_RUST_ROOT", temp.path())
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert!(!output.join("manifest.json").exists());
    assert!(!output.join("policy").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn final_result_write_interruption_removes_sdk_success_manifest() {
    use rustix::{
        fs::{mkfifoat, open, Mode, OFlags, CWD},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use serde_json::Value;
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let required = |name| {
        fs::canonicalize(std::env::var_os(name).expect("mandatory native artifact")).unwrap()
    };
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("runtime");
    let stderr = temp.path().join("checker.stderr");
    let child = Command::new(env!("CARGO_BIN_EXE_check-crash-sdk-transport"))
        .arg(required("TOMBSTONE_PROBE_BINARY"))
        .arg(required("TOMBSTONE_PARAMS_BINDING"))
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", &root)
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(fs::File::create(temp.path().join("checker.stdout")).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker { child, identity };
    let deadline = Instant::now() + Duration::from_secs(90);
    let ready = output.join("policy/comparisons.jsonl");
    loop {
        if fs::metadata(&ready).is_ok_and(|metadata| metadata.len() > 0) {
            break;
        }
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "SDK checker exited before its first comparison: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "SDK checker did not begin actual comparisons"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let fifo = output.join("manifest.json");
    mkfifoat(CWD, &fifo, Mode::RUSR | Mode::WUSR).unwrap();
    let wchan = format!("/proc/{}/wchan", checker.child.id());
    // The main thread is blocked opening our FIFO after all14 real SDK events and native IO exception.
    // No guessed delay or production hook chooses this final write boundary.
    loop {
        if fs::read_to_string(&wchan).is_ok_and(|state| state.trim() == "wait_for_partner") {
            break;
        }
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "SDK checker exited before final writer boundary: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "checker never reached final manifest FIFO"
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
            Err(error) => panic!("manifest FIFO read failed: {error}"),
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
    let report: Value = serde_json::from_slice(&document).unwrap();
    assert_eq!(report["result"], "PASS");
    assert_eq!(report["events"], 14);
    assert_eq!(report["native_io_exception_events"], 1);
    assert!(!status.success(), "interrupted checker exited successfully");
    assert!(
        !output.join("manifest.json").exists(),
        "interrupted final writer left its success manifest visible"
    );
}

#[test]
fn all_four_partial_helpers_may_be_absent_with_exact_historical_sdk_provenance() {
    use serde_json::Value;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let copied = tempfile::tempdir().unwrap();
    let retired = [
        (
            "rust/tools/check_crash_sdk_transport.py",
            "8bf1281bbc182ecaba9a29b9e49aa3d250ec8ee1f43eca5861596cee475ed806",
        ),
        (
            "rust/tools/check_tombstoned_reference.py",
            "6271ece66356f377357ffc2b37bf3d98fcdb4c8231c8c07f556ed0294c261cd4",
        ),
        (
            "rust/tools/tombstoned_reference.py",
            "7b647dba322d93e5d2bd9c5692356b10d3f85f2a2e0f9da8b982e05206c8e781",
        ),
        (
            "rust/tools/check_version_reference.py",
            "d0ad5fbec23e1ba4a2507ae6dbc68856141d03eda07dadebab3280d5731525d5",
        ),
    ];
    let pins = openpilot_dev_tools::tombstone_source::sources(&repository).unwrap();
    for name in pins
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("openpilot/common/params_pyx.pyx"))
    {
        if retired.iter().any(|(retired, _)| *retired == name) {
            continue;
        }
        let path = copied.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(repository.join(name), path).unwrap();
    }
    let helper = "rust/crates/dev-tools/src/tombstone_reference_check/apport-retrace";
    let helper_path = copied.path().join(helper);
    fs::create_dir_all(helper_path.parent().unwrap()).unwrap();
    fs::copy(repository.join(helper), helper_path).unwrap();
    let required = |name| {
        fs::canonicalize(std::env::var_os(name).expect("mandatory native artifact")).unwrap()
    };
    let options = Options {
        binary: required("TOMBSTONE_PROBE_BINARY"),
        params_binding: required("TOMBSTONE_PARAMS_BINDING"),
        runner: vec![],
        output: copied.path().join("sdk"),
        environment: std::env::vars_os().collect(),
    };
    let report = sdk_transport_check::run(copied.path(), &options).unwrap();
    let report: Value = serde_json::from_str(&report.to_json().unwrap()).unwrap();
    assert_eq!(report["result"], "PASS");
    assert_eq!(report["events"], 14);
    assert_eq!(report["native_io_exception_events"], 1);
    for (name, hash) in retired {
        assert!(report["source_hashes"].get(name).is_none());
        assert_eq!(report["historical_source_sha256"][name], hash);
        let path = copied.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"changed retired helper\n").unwrap();
        assert!(sdk_transport_check::run(copied.path(), &options).is_err());
        assert!(!options.output.join("manifest.json").exists());
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink("missing-retired", &path).unwrap();
        assert!(sdk_transport_check::run(copied.path(), &options).is_err());
        assert!(!options.output.join("manifest.json").exists());
        fs::remove_file(&path).unwrap();
    }
}
