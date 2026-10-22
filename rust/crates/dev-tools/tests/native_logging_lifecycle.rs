#![cfg(target_os = "linux")]
//! Explicit Actions-only actual-provider controls; never run by a local build.
use rustix::process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    os::{
        fd::OwnedFd,
        unix::{
            fs::{symlink, MetadataExt},
            net::{UnixDatagram, UnixListener},
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const CHECKER: &str = env!("CARGO_BIN_EXE_check-native-logging");
fn root() -> PathBuf {
    let path = std::env::var_os("NATIVE_LOGGING_LIFECYCLE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    fs::create_dir_all(&path).unwrap();
    tempfile::Builder::new()
        .prefix("logging-lifecycle-")
        .tempdir_in(path)
        .unwrap()
        .keep()
}
fn provider(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("Actions must set {name}")))
        .canonicalize()
        .unwrap()
}
fn resume_third_run() -> bool {
    match std::env::var("NATIVE_LOGGING_LIFECYCLE_RESUME") {
        Err(std::env::VarError::NotPresent) => false,
        Ok(value) if value == "39833849895" => true,
        value => panic!("unknown logging lifecycle resume receipt: {value:?}"),
    }
}
fn command(output: &Path) -> Command {
    command_reference(output, true)
}
fn command_reference(output: &Path, prebuilt: bool) -> Command {
    let mut command = Command::new(CHECKER);
    command
        .arg("--binary")
        .arg(provider("NATIVE_LOGGING_TEST_BINARY"))
        .arg("--builder")
        .arg(provider("NATIVE_LOGGING_TEST_BUILDER"))
        .arg("--root")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
        .arg("--output")
        .arg(output);
    if prebuilt {
        command
            .arg("--reference")
            .arg(provider("NATIVE_LOGGING_TEST_REFERENCE"));
    }
    command
}
struct Checker {
    child: Option<Child>,
    identity: OwnedFd,
    peer: Option<OwnedFd>,
    stderr: PathBuf,
}
impl Checker {
    fn wait(&mut self, timeout: Duration, stage: &str, mut condition: impl FnMut() -> bool) {
        let until = Instant::now() + timeout;
        while !condition() {
            let running = self.child.as_mut().unwrap().try_wait().unwrap().is_none();
            assert!(
                running && Instant::now() < until,
                "{stage}: running={running}; stderr={}",
                fs::read_to_string(&self.stderr).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn finish(&mut self) -> std::process::ExitStatus {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                self.child = None;
                return status;
            }
            assert!(
                Instant::now() < until,
                "checker shutdown exceeded bound; {}",
                fs::read_to_string(&self.stderr).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for Checker {
    fn drop(&mut self) {
        if let Some(peer) = &self.peer {
            let _ = pidfd_send_signal(peer, Signal::KILL);
        }
        if let Some(child) = self.child.as_mut() {
            let _ = pidfd_send_signal(&self.identity, Signal::TERM);
            let until = Instant::now() + Duration::from_secs(20);
            while Instant::now() < until {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let _ = pidfd_send_signal(&self.identity, Signal::KILL);
            // The test harness has its own bound even when cleanup itself fails.
            let until = Instant::now() + Duration::from_secs(2);
            while Instant::now() < until {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            eprintln!(
                "lifecycle test could not reap checker; diagnostics at {}",
                self.stderr.display()
            );
        }
    }
}
fn held(mode: &str) -> (PathBuf, PathBuf, Checker, Value) {
    let root = root();
    let output = root.join("output");
    let records = root.join("records");
    let stderr = root.join("checker.stderr");
    let child = command(&output)
        .env("NATIVE_LOGGING_QA_HOLD", mode)
        .env("NATIVE_LOGGING_QA_RECORDS", &records)
        .stdout(Stdio::null())
        .stderr(File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker {
        child: Some(child),
        identity,
        peer: None,
        stderr,
    };
    checker.wait(
        Duration::from_secs(if mode == "runtime" { 60 } else { 15 }),
        "owned actual producer barrier",
        || records.join("held.json").is_file(),
    );
    let held: Value =
        serde_json::from_slice(&fs::read(records.join("held.json")).unwrap()).unwrap();
    let pid = held["pid"].as_u64().unwrap() as i32;
    checker.peer = Some(pidfd_open(Pid::from_raw(pid).unwrap(), PidfdFlags::empty()).unwrap());
    (root, output, checker, held)
}
fn paths(mode: &str, output: &Path, held: &Value) -> (PathBuf, PathBuf) {
    if mode == "runtime" {
        let row: Value = serde_json::from_slice(
            &fs::read(output.join("transport/runtime-endpoint.json")).unwrap(),
        )
        .unwrap();
        (
            PathBuf::from(row["path"].as_str().unwrap()),
            PathBuf::from(row["private_marker"].as_str().unwrap()),
        )
    } else {
        let path = PathBuf::from(
            held["endpoint"]
                .as_str()
                .unwrap()
                .strip_prefix("ipc://")
                .unwrap(),
        );
        let parent = path.parent().unwrap().to_path_buf();
        (path, parent)
    }
}
struct Foreign {
    path: PathBuf,
    dev: u64,
    inode: u64,
}
impl Foreign {
    fn new(path: &Path, kind: &str) -> Self {
        fs::remove_file(path).unwrap();
        match kind {
            "regular" => fs::write(path, b"foreign replacement").unwrap(),
            "datagram" => drop(UnixDatagram::bind(path).unwrap()),
            "closed-stream" => drop(UnixListener::bind(path).unwrap()),
            "symlink" => symlink("missing-foreign-target", path).unwrap(),
            "directory" => fs::create_dir(path).unwrap(),
            _ => panic!("unknown replacement"),
        }
        let metadata = fs::symlink_metadata(path).unwrap();
        Self {
            path: path.into(),
            dev: metadata.dev(),
            inode: metadata.ino(),
        }
    }
    fn assert_preserved(&self, kind: &str) {
        let metadata = fs::symlink_metadata(&self.path).unwrap();
        assert_eq!((metadata.dev(), metadata.ino()), (self.dev, self.inode));
        if kind == "regular" {
            assert_eq!(fs::read(&self.path).unwrap(), b"foreign replacement");
        }
        if kind == "symlink" {
            assert_eq!(
                fs::read_link(&self.path).unwrap(),
                Path::new("missing-foreign-target")
            );
        }
    }
}
impl Drop for Foreign {
    fn drop(&mut self) {
        if let Ok(metadata) = fs::symlink_metadata(&self.path) {
            if (metadata.dev(), metadata.ino()) == (self.dev, self.inode) {
                if metadata.is_dir() {
                    let _ = fs::remove_dir(&self.path);
                } else {
                    let _ = fs::remove_file(&self.path);
                }
            }
        }
    }
}
fn record(root: &Path, held: &Value, code: i32, mode: &str, kind: &str, seconds: f64) {
    fs::write(
        root.join("case.json"),
        serde_json::to_vec_pretty(&json!({
            "mode":mode,"replacement":kind,"returncode":code,"held":held,
        "actual_producer_reaped":true,"owned_directory_removed":true,"shutdown_seconds":seconds
        }))
        .unwrap(),
    )
    .unwrap();
}
#[test]
fn argument_errors_precede_any_child_execution() {
    let result = Command::new(CHECKER).arg("--unknown").output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("required"));
    assert!(Command::new(CHECKER)
        .arg("--help")
        .status()
        .unwrap()
        .success());
}
#[test]
#[ignore = "requires actual independently built C++ and Rust logging providers"]
fn actual_cli_int_term_reap_held_producer_and_preserve_runtime_foreign_replacement() {
    let resume = resume_third_run();
    // Two initial-C++ cases + twelve runtime-Rust cases. Default runtime paths
    // are checked both with the original node and with every foreign node kind.
    for mode in ["first", "runtime"] {
        for kind in [
            "owned",
            "regular",
            "datagram",
            "closed-stream",
            "symlink",
            "directory",
        ] {
            if mode == "first" && kind != "owned" {
                continue;
            }
            if resume && (mode != "runtime" || kind != "directory") {
                continue;
            }
            for signal in [Signal::INT, Signal::TERM] {
                let (root, output, mut checker, held) = held(mode);
                pidfd_send_signal(checker.peer.as_ref().unwrap(), Signal::STOP).unwrap();
                let (path, private) = paths(mode, &output, &held);
                let foreign = (kind != "owned").then(|| Foreign::new(&path, kind));
                let shutdown = Instant::now();
                pidfd_send_signal(&checker.identity, signal).unwrap();
                assert_eq!(checker.finish().code(), Some(128 + signal.as_raw()));
                let pid = held["pid"].as_u64().unwrap();
                assert!(
                    !Path::new(&format!("/proc/{pid}")).exists(),
                    "actual producer not reaped"
                );
                assert!(
                    !private.exists(),
                    "owned directory retained after confirmed cleanup"
                );
                if let Some(foreign) = &foreign {
                    foreign.assert_preserved(kind);
                } else {
                    assert!(!path.exists());
                }
                record(
                    &root,
                    &held,
                    128 + signal.as_raw(),
                    mode,
                    kind,
                    shutdown.elapsed().as_secs_f64(),
                );
            }
        }
    }
}
#[test]
#[ignore = "requires actual independently built C++ and Rust logging providers"]
fn actual_cli_eof_failure_cleans_receiver_before_return() {
    // Kill a held actual producer before any command, then release the checker.
    // Both the C++ initial receiver and shared runtime receiver take real EOF.
    for mode in ["first", "runtime"] {
        for kind in ["owned", "regular"] {
            if mode == "first" && kind != "owned" {
                continue;
            }
            let (root, output, mut checker, held) = held(mode);
            let (path, private) = paths(mode, &output, &held);
            let foreign = (kind != "owned").then(|| Foreign::new(&path, kind));
            let shutdown = Instant::now();
            pidfd_send_signal(checker.peer.as_ref().unwrap(), Signal::KILL).unwrap();
            fs::write(
                root.join("records/release"),
                b"release after actual producer kill",
            )
            .unwrap();
            assert_eq!(checker.finish().code(), Some(1));
            let stderr = fs::read_to_string(&checker.stderr).unwrap();
            assert!(
                stderr.contains("closed stderr")
                    || stderr.contains("Broken pipe")
                    || stderr.contains("broken pipe"),
                "{stderr}"
            );
            let pid = held["pid"].as_u64().unwrap();
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
            assert!(!private.exists());
            if let Some(foreign) = &foreign {
                foreign.assert_preserved(kind);
            } else {
                assert!(!path.exists());
            }
            record(
                &root,
                &held,
                1,
                mode,
                kind,
                shutdown.elapsed().as_secs_f64(),
            );
        }
    }
}

fn node_snapshot(path: &Path) -> Value {
    let metadata = fs::symlink_metadata(path).unwrap();
    let contents = if metadata.is_dir() {
        let mut entries = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        json!(entries.iter().map(|entry| json!({"name":entry.file_name().unwrap().to_string_lossy(),"node":node_snapshot(entry)})).collect::<Vec<_>>())
    } else if metadata.is_file() {
        json!(fs::read(path).unwrap())
    } else if metadata.file_type().is_symlink() {
        json!(fs::read_link(path).unwrap())
    } else {
        Value::Null
    };
    json!({"dev":metadata.dev(),"inode":metadata.ino(),"mode":metadata.mode(),"size":metadata.len(),
        "mtime":metadata.mtime(),"mtime_nsec":metadata.mtime_nsec(),"ctime":metadata.ctime(),"ctime_nsec":metadata.ctime_nsec(),"contents":contents})
}
#[test]
#[ignore = "requires the actual held CLI and provider paths in hosted Actions"]
fn actual_cli_foreign_output_collisions_precede_builder_or_provider_work() {
    let resume = resume_third_run();
    for prebuilt in [false, true] {
        for kind in [
            "file",
            "directory",
            "live-directory",
            "symlink",
            "dangling-symlink",
        ] {
            if resume && !prebuilt && matches!(kind, "file" | "directory") {
                continue;
            }
            let root = root();
            let output = root.join("output");
            let external = root.join("external");
            fs::create_dir(&external).unwrap();
            fs::write(external.join("sentinel"), b"external foreign bytes").unwrap();
            match kind {
                "file" => fs::write(&output, b"foreign output file").unwrap(),
                "directory" | "live-directory" => {
                    fs::create_dir(&output).unwrap();
                    fs::write(output.join("sentinel"), b"foreign output directory").unwrap();
                    fs::write(output.join("report.json"), b"foreign report").unwrap();
                    fs::write(
                        output.join("providers.after.json"),
                        b"foreign provider receipt",
                    )
                    .unwrap();
                    symlink(
                        external.join("sentinel"),
                        output.join("providers.before.json"),
                    )
                    .unwrap();
                }
                "symlink" => symlink(&external, &output).unwrap(),
                _ => symlink(root.join("missing"), &output).unwrap(),
            }
            let _live =
                (kind == "live-directory").then(|| UnixListener::bind(output.join("s")).unwrap());
            let before =
                json!({"output":node_snapshot(&output),"external":node_snapshot(&external)});
            let stderr = root.join("checker.stderr");
            let child = command_reference(&output, prebuilt)
                .env("NATIVE_LOGGING_QA_HOLD", "first")
                .env("NATIVE_LOGGING_QA_RECORDS", root.join("unexpected-child"))
                .stdout(File::create(root.join("checker.stdout")).unwrap())
                .stderr(File::create(&stderr).unwrap())
                .spawn()
                .unwrap();
            let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
            let mut checker = Checker {
                child: Some(child),
                identity,
                peer: None,
                stderr,
            };
            assert_eq!(checker.finish().code(), Some(1));
            let stderr = fs::read_to_string(&checker.stderr).unwrap();
            assert!(
                stderr.contains("logging evidence output already exists"),
                "{stderr}"
            );
            let after =
                json!({"output":node_snapshot(&output),"external":node_snapshot(&external)});
            assert_eq!(before, after);
            assert!(!root.join("unexpected-child").exists());
            assert!(fs::read(root.join("checker.stdout")).unwrap().is_empty());
            fs::write(
                root.join("collision-case.json"),
                serde_json::to_vec_pretty(&json!({
                    "kind":kind,"prebuilt":prebuilt,"returncode":1,"before":before,"after":after,
                    "producer_barrier_absent":true
                }))
                .unwrap(),
            )
            .unwrap();
        }
    }
}
