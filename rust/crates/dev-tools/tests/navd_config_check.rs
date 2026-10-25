//! Native API RED from unchanged original checker contracts, no native oracle.
#[path = "support/registration_source_pins.rs"]
mod source_pins;
use openpilot_dev_tools::navd_config_check::{self as native, Command, Options};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ffi::OsString, fs, os::unix::fs::symlink, path::PathBuf};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn fixture(n: &str) -> Value {
    serde_json::from_slice(
        &fs::read(
            root()
                .join("rust/crates/dev-tools/tests/fixtures/navd-config")
                .join(n),
        )
        .unwrap(),
    )
    .unwrap()
}
fn required(name: &str) -> PathBuf {
    let p = PathBuf::from(std::env::var_os(name).unwrap_or_else(|| {
        panic!("prerequisite: {name} must name an actual fresh source-matched ELF")
    }));
    assert!(p.is_absolute() && p.is_file());
    p
}
#[test]
fn immutable_original_manifest_binds_full_roster_generic_cli_and_source_without_native_expectations(
) {
    let p = root().join("rust/crates/dev-tools/tests/fixtures/navd-config/manifest.json");
    let bytes = fs::read(p).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "648fb67f5662477f0de52466751f5821fab3a9e3de49e44b5cfa81b7d50ede34"
    );
    let m: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(m["candidate_executed_for_expectations"], false);
    assert_eq!(
        m["counts"],
        serde_json::json!({"roster":26,"generic":8,"cli":12})
    );
    assert_eq!(m["source_sha256"].as_object().unwrap().len(), 30);
    for (n, h) in m["fixtures"].as_object().unwrap() {
        let b = fs::read(
            root()
                .join("rust/crates/dev-tools/tests/fixtures/navd-config")
                .join(n),
        )
        .unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(b)), h.as_str().unwrap());
    }
    let rows = fixture("source-original.json");
    let roster = native::cases().unwrap();
    assert_eq!(roster.len(), 26);
    for (case, expected) in roster.iter().zip(rows["rows"].as_array().unwrap().iter()) {
        assert_eq!(case.name, expected["case"].as_str().unwrap());
        assert_eq!(
            serde_json::from_str::<Value>(&case.input.to_json().unwrap()).unwrap(),
            expected["input"]
        );
    }
}
#[test]
fn native_grammar_preserves_original_admission_and_explicitly_removes_only_binding() {
    let cwd = root().canonicalize().unwrap();
    let env = BTreeMap::new();
    for args in [vec!["--help"], vec!["-h"], vec!["-hfoo"], vec!["--he"]] {
        let args = args.into_iter().map(OsString::from).collect::<Vec<_>>();
        assert!(matches!(
            native::parse(&args, &cwd, env.clone()).unwrap(),
            Command::Help
        ));
    }
    for args in [
        vec![],
        vec!["--binary"],
        vec!["--binary", "missing"],
        vec!["--unknown"],
    ] {
        let a = args.into_iter().map(OsString::from).collect::<Vec<_>>();
        assert!(native::parse(&a, &cwd, env.clone()).is_err());
    }
    let args = ["--binary", "relative-probe", "--output", "relative-output"].map(OsString::from);
    let Command::Run(o) = native::parse(&args, &cwd, env).unwrap() else {
        panic!("run expected")
    };
    assert_eq!(o.binary, cwd.join("relative-probe"));
    assert_eq!(o.output, cwd.join("relative-output"));
}
#[test]
fn source_pins_reject_changed_missing_and_dangling_inputs_before_probe_execution() {
    let t = tempfile::tempdir().unwrap();
    let m = fixture("manifest.json");
    for (n, _) in m["source_sha256"].as_object().unwrap() {
        let to = t.path().join(n);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        if let Some(raw) = source_pins::read(&root(), n) {
            fs::write(to, raw).unwrap();
        }
    }
    native::validate_source(t.path()).unwrap();
    for (n, _) in m["source_sha256"].as_object().unwrap() {
        let p = t.path().join(n);
        let Some(bytes) = source_pins::read(t.path(), n) else {
            continue;
        };
        fs::write(&p, b"changed").unwrap();
        assert!(native::validate_source(t.path()).is_err(), "changed {n}");
        fs::remove_file(&p).unwrap();
        assert_eq!(
            native::validate_source(t.path()).is_ok(),
            source_pins::retired(n),
            "missing {n}"
        );
        symlink(t.path().join("absent-target"), &p).unwrap();
        assert!(native::validate_source(t.path()).is_err(), "dangling {n}");
        fs::remove_file(&p).unwrap();
        fs::write(p, bytes).unwrap();
    }
}
#[test]
fn actual_full26_report_and_foreign_output_receipt_keep_exclusive_ownership() {
    let t = tempfile::tempdir().unwrap();
    let output = t.path().join("actual");
    let o = Options {
        root: root(),
        binary: required("NAVD_CONFIG_BINARY"),
        output: output.clone(),
        environment: std::env::vars_os().collect(),
    };
    let result = native::run(&o, &required("NAVD_CONFIG_WORKER_BINARY")).unwrap();
    let report: Value = serde_json::from_str(&result.to_json().unwrap()).unwrap();
    assert_eq!(report["status"], "PASS");
    assert_eq!(report["cases"].as_array().unwrap().len(), 26);
    for row in report["cases"].as_array().unwrap() {
        assert_eq!(row["passed"], true);
    }
    let existing = t.path().join("foreign");
    fs::create_dir(&existing).unwrap();
    let sentinel = existing.join("report.json");
    fs::write(&sentinel, b"{\"status\":\"PASS\",\"foreign\":true}\n").unwrap();
    let o = Options {
        output: existing,
        ..o
    };
    assert!(native::run(&o, &required("NAVD_CONFIG_WORKER_BINARY")).is_err());
    assert_eq!(
        fs::read(sentinel).unwrap(),
        b"{\"status\":\"PASS\",\"foreign\":true}\n"
    );
}
#[test]
fn missing_candidate_preserves_original_source_first_artifacts_without_success_receipt() {
    let t = tempfile::tempdir().unwrap();
    let output = t.path().join("missing-native");
    let o = Options {
        root: root(),
        binary: t.path().join("missing-probe"),
        output: output.clone(),
        environment: std::env::vars_os().collect(),
    };
    assert!(native::run(&o, &required("NAVD_CONFIG_WORKER_BINARY")).is_err());
    let record: Value = serde_json::from_slice(
        &fs::read(output.join("environment/source/record.json"))
            .expect("unchanged source-before-candidate phase"),
    )
    .unwrap();
    assert_eq!(
        record,
        serde_json::json!({"ok":true,"host":"https://api.mapbox.com","token":"override-token"})
    );
    assert!(output.join("environment/source/stdout").is_file());
    assert!(output.join("environment/source/stderr").is_file());
    assert!(!output.join("report.json").exists(), "no stale PASS");
}

#[test]
fn original_bad_stdout_failure_report_and_fifteen_second_timeout_keep_artifact_phases() {
    let fixture_path =
        root().join("rust/crates/dev-tools/tests/fixtures/navd-config/process-original.json");
    let bytes = fs::read(fixture_path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "d661c29eb60eb11acaa561f1efb7673310e87d5b44c91dac77ab37b9f8096521"
    );
    let expected: Value = serde_json::from_slice(&bytes).unwrap();
    let t = tempfile::tempdir().unwrap();
    let worker = required("NAVD_CONFIG_WORKER_BINARY");
    for (name, binary) in [
        ("empty-success", PathBuf::from("/usr/bin/true")),
        ("all-false", PathBuf::from("/usr/bin/false")),
        (
            "timeout",
            root().join("rust/crates/dev-tools/tests/fixtures/navd-config/slow-probe.sh"),
        ),
    ] {
        let row = expected["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .unwrap();
        let output = t.path().join(name);
        let options = Options {
            root: root(),
            binary,
            output: output.clone(),
            environment: std::env::vars_os().collect(),
        };
        let started = std::time::Instant::now();
        let result = native::run(&options, &worker);
        if name == "all-false" {
            let report: Value = serde_json::from_str(&result.unwrap().to_json().unwrap()).unwrap();
            assert_eq!(report["status"], row["report_status"]);
            assert_eq!(report["cases"], row["cases"]);
        } else {
            assert!(result.is_err());
        }
        if name == "timeout" {
            assert!(started.elapsed() >= std::time::Duration::from_secs(15));
        }
        assert_eq!(
            output.join("environment/source/record.json").exists(),
            row["source_first"].as_bool().unwrap()
        );
        assert_eq!(
            output.join("environment/native/stdout").exists(),
            row["native_stdout"].as_bool().unwrap()
        );
        assert_eq!(
            output.join("environment/native/record.json").exists(),
            row["native_record"].as_bool().unwrap()
        );
        assert_eq!(
            output.join("report.json").exists(),
            row["has_report"].as_bool().unwrap()
        );
    }
}

#[test]
fn actual_cli_interrupt_after_child_start_reaps_probe_and_retains_original_signal_status() {
    use rustix::process::{Pid, Signal, WaitId, WaitIdOptions};
    use std::{
        cell::Cell,
        io::Read,
        os::unix::process::{CommandExt, ExitStatusExt},
        path::Path,
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    fn required_diagnostics() -> PathBuf {
        let path = PathBuf::from(
            std::env::var_os("NAVD_CONFIG_CANCEL_DIAGNOSTICS")
                .expect("NAVD_CONFIG_CANCEL_DIAGNOSTICS must name a fresh evidence directory"),
        );
        assert!(path.is_absolute(), "diagnostics path must be absolute");
        path
    }
    fn copy_evidence(source: &Path, destination: &Path) -> std::io::Result<()> {
        if !source.exists() {
            return Ok(());
        }
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let path = entry.path();
            let target = destination.join(entry.file_name());
            let kind = entry.file_type()?;
            if kind.is_dir() {
                copy_evidence(&path, &target)?;
            } else if kind.is_file() {
                let size = entry.metadata()?.len();
                if size > 8 * 1024 * 1024 {
                    return Err(std::io::Error::other(format!(
                        "diagnostic file exceeds 8 MiB: {} ({size} bytes)",
                        path.display()
                    )));
                }
                fs::copy(path, target)?;
            } else {
                return Err(std::io::Error::other(format!(
                    "unexpected diagnostic entry: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }
    fn capture_proc(pid: Pid, destination: &Path) -> Vec<String> {
        let source = PathBuf::from(format!("/proc/{}", pid.as_raw_pid()));
        let mut errors = Vec::new();
        for (name, limit) in [
            ("stat", 16 * 1024),
            ("status", 64 * 1024),
            ("wchan", 16 * 1024),
            ("cmdline", 64 * 1024),
        ] {
            let result = (|| -> std::io::Result<()> {
                let mut bytes = Vec::new();
                fs::File::open(source.join(name))?
                    .take(limit + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() as u64 > limit {
                    return Err(std::io::Error::other(format!(
                        "/proc/{name} exceeds {limit} bytes"
                    )));
                }
                fs::write(destination.join(format!("controller.proc.{name}")), bytes)
            })();
            if let Err(error) = result {
                errors.push(format!("{name}: {error}"));
            }
        }
        errors
    }
    struct Diagnostics {
        destination: PathBuf,
        output: PathBuf,
        marker: PathBuf,
        stdout: PathBuf,
        stderr: PathBuf,
        pid: Pid,
        started: Instant,
        phase: Cell<&'static str>,
        controller_signal: Cell<Option<i32>>,
        probe_pid: Cell<Option<i32>>,
    }
    impl Drop for Diagnostics {
        fn drop(&mut self) {
            let controller_alive = rustix::process::waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .map(|status| status.is_none())
            .ok();
            let proc_errors =
                if std::thread::panicking() && self.phase.get() == "waiting_for_candidate_start" {
                    capture_proc(self.pid, &self.destination)
                } else {
                    Vec::new()
                };
            let copied = copy_evidence(&self.output, &self.destination.join("actual"));
            let _ = fs::copy(&self.stdout, self.destination.join("controller.stdout"));
            let _ = fs::copy(&self.stderr, self.destination.join("controller.stderr"));
            if self.marker.exists() {
                let _ = fs::copy(&self.marker, self.destination.join("marker"));
            }
            let record = self.output.join("environment/source/record.json");
            let receipt = self.output.join("report.json");
            let state = serde_json::json!({
                "phase": self.phase.get(),
                "panicking": std::thread::panicking(),
                "elapsed_ms": self.started.elapsed().as_millis(),
                "controller_alive": controller_alive,
                "marker_exists": self.marker.exists(),
                "source_record_exists": record.is_file(),
                "report_exists": receipt.is_file(),
                "controller_signal": self.controller_signal.get(),
                "probe_pid": self.probe_pid.get(),
                "probe_alive": self.probe_pid.get().map(|pid| PathBuf::from(format!("/proc/{pid}")).exists()),
                "output_copy_error": copied.err().map(|error| error.to_string()),
                "proc_snapshot_errors": proc_errors,
            });
            let _ = fs::write(
                self.destination.join("state.json"),
                serde_json::to_vec_pretty(&state).unwrap(),
            );
        }
    }
    struct Owned(Option<Child>);
    impl Drop for Owned {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                let pid = Pid::from_raw(child.id() as i32).unwrap();
                let _ = rustix::process::kill_process_group(pid, Signal::KILL);
                let _ = child.wait();
            }
        }
    }
    let t = tempfile::tempdir().unwrap();
    let marker = t.path().join("marker");
    let output = t.path().join("actual");
    let stdout = t.path().join("controller.stdout");
    let stderr = t.path().join("controller.stderr");
    let destination = required_diagnostics();
    fs::create_dir(&destination)
        .expect("NAVD_CONFIG_CANCEL_DIAGNOSTICS must name a fresh directory");
    let trace = destination.join("controller-phases");
    fs::create_dir(&trace).unwrap();
    let mut command = Command::new(required("NAVD_CONFIG_WORKER_BINARY"));
    command.args([
        std::ffi::OsStr::new("--binary"),
        root()
            .join("rust/crates/dev-tools/tests/fixtures/navd-config/slow-probe.sh")
            .as_os_str(),
        std::ffi::OsStr::new("--output"),
        output.as_os_str(),
    ]);
    command
        .env("NAVD_CONFIG_FIXTURE_MARKER", &marker)
        .env("NAVD_CONFIG_STARTUP_TRACE", &trace)
        .env("OPENPILOT_RUST_ROOT", root())
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .process_group(0);
    let mut child = Owned(Some(command.spawn().unwrap()));
    let pid = Pid::from_raw(child.0.as_ref().unwrap().id() as i32).unwrap();
    let diagnostics = Diagnostics {
        destination,
        output: output.clone(),
        marker: marker.clone(),
        stdout,
        stderr,
        pid,
        started: Instant::now(),
        phase: Cell::new("waiting_for_candidate_start"),
        controller_signal: Cell::new(None),
        probe_pid: Cell::new(None),
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "actual child start barrier");
        assert!(rustix::process::waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT
        )
        .unwrap()
        .is_none());
        std::thread::sleep(Duration::from_millis(5));
    }
    let probe: i32 = fs::read_to_string(&marker).unwrap().trim().parse().unwrap();
    diagnostics.probe_pid.set(Some(probe));
    diagnostics.phase.set("waiting_for_interrupt_cleanup");
    rustix::process::kill_process(pid, Signal::INT).unwrap();
    while rustix::process::waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )
    .unwrap()
    .is_none()
    {
        assert!(Instant::now() < deadline, "interrupted controller cleanup");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !PathBuf::from(format!("/proc/{probe}")).exists(),
        "controller itself must reap the actual child before test cleanup"
    );
    let status = child.0.take().unwrap().wait().unwrap();
    diagnostics.controller_signal.set(status.signal());
    assert_eq!(status.signal(), Some(2));
    assert!(output.join("environment/source/record.json").is_file());
    assert!(!output.join("report.json").exists());
    diagnostics.phase.set("passed");
}

#[test]
fn closed_stdout_after_all_real_comparisons_cannot_leave_a_success_report() {
    use rustix::process::{Pid, Signal, WaitId, WaitIdOptions};
    use std::{
        os::{
            fd::OwnedFd,
            unix::{net::UnixStream, process::CommandExt},
        },
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    struct Owned(Option<Child>);
    impl Drop for Owned {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                let pid = Pid::from_raw(child.id() as i32).unwrap();
                let _ = rustix::process::kill_process_group(pid, Signal::KILL);
                let _ = child.wait();
            }
        }
    }
    let t = tempfile::tempdir().unwrap();
    let output = t.path().join("actual");
    let stderr = t.path().join("stderr");
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);
    let mut command = Command::new(required("NAVD_CONFIG_WORKER_BINARY"));
    command
        .arg("--binary")
        .arg(required("NAVD_CONFIG_BINARY"))
        .arg("--output")
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", root())
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(fs::File::create(&stderr).unwrap())
        .process_group(0);
    let mut child = Owned(Some(command.spawn().unwrap()));
    let pid = Pid::from_raw(child.0.as_ref().unwrap().id() as i32).unwrap();
    let deadline = Instant::now() + Duration::from_secs(90);
    while rustix::process::waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )
    .unwrap()
    .is_none()
    {
        assert!(
            Instant::now() < deadline,
            "full actual comparison must finish"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let status = child.0.take().unwrap().wait().unwrap();
    assert!(
        !status.success(),
        "closed stdout must be an actual command failure"
    );
    let cases = native::cases().unwrap();
    assert_eq!(cases.len(), 26);
    assert_eq!(cases.last().unwrap().name, "integer-9");
    for case in cases {
        for side in ["source", "native"] {
            assert!(
                output
                    .join(&case.name)
                    .join(side)
                    .join("record.json")
                    .is_file(),
                "full workload required: {} {side}",
                case.name
            );
        }
    }
    assert!(
        !output.join("report.json").exists(),
        "failed stdout publication left a new success receipt; stderr={}",
        fs::read_to_string(&stderr).unwrap()
    );
    assert!(!fs::read_to_string(stderr).unwrap().contains("panicked"));
}
