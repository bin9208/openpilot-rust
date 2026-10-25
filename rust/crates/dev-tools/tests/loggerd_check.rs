#![cfg(target_os = "linux")]
use rustix::process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal};
use serde_json::Value;
use std::os::fd::OwnedFd;
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
fn evidence() -> PathBuf {
    let parent = std::env::var_os("LOGGERD_LIFECYCLE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    fs::create_dir_all(&parent).unwrap();
    tempfile::Builder::new()
        .prefix("loggerd-lifecycle-")
        .tempdir_in(parent)
        .unwrap()
        .keep()
}
const CHECKER: &str = env!("CARGO_BIN_EXE_check-loggerd-native");
const FIXTURE: &str = env!("CARGO_BIN_EXE_loggerd-lifecycle-fixture");
struct Checker {
    child: Option<Child>,
    identity: OwnedFd,
    peers: Vec<OwnedFd>,
    stderr: PathBuf,
}
impl Checker {
    fn wait(&mut self, stage: &str, mut ready: impl FnMut() -> bool) {
        let until = Instant::now() + Duration::from_secs(15);
        while !ready() {
            let running = self.child.as_mut().unwrap().try_wait().unwrap().is_none();
            assert!(
                running && Instant::now() < until,
                "{stage}: checker_running={running}; stderr={}",
                fs::read_to_string(&self.stderr).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn finish(&mut self) -> std::process::ExitStatus {
        let until = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                self.child = None;
                return status;
            }
            assert!(
                Instant::now() < until,
                "checker cleanup exceeded bound: {}",
                fs::read_to_string(&self.stderr).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Checker {
    fn drop(&mut self) {
        for peer in &self.peers {
            let _ = pidfd_send_signal(peer, Signal::KILL);
        }
        if self.child.is_some() {
            let _ = pidfd_send_signal(&self.identity, Signal::TERM);
            let until = Instant::now() + Duration::from_secs(15);
            while Instant::now() < until {
                if self
                    .child
                    .as_mut()
                    .unwrap()
                    .try_wait()
                    .ok()
                    .flatten()
                    .is_some()
                {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let _ = pidfd_send_signal(&self.identity, Signal::KILL);
            let _ = self.child.as_mut().unwrap().wait();
        }
    }
}
fn command(output: &Path) -> Command {
    let mut c = Command::new(CHECKER);
    c.args([
        "--binary",
        FIXTURE,
        "--original",
        FIXTURE,
        "--producer",
        FIXTURE,
        "--scenario",
        "ordinary",
        "--output",
    ])
    .arg(output);
    c
}
#[test]
fn help_and_required_argument_errors_do_not_spawn_or_create_output() {
    assert!(Command::new(CHECKER)
        .arg("--help")
        .status()
        .unwrap()
        .success());
    let r = Command::new(CHECKER).arg("--unknown").output().unwrap();
    assert_eq!(r.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&r.stderr).contains("required"));
}
#[test]
fn existing_output_is_rejected_without_clobbering_foreign_evidence() {
    let t = evidence();
    let out = t.as_path().join("occupied");
    fs::create_dir(&out).unwrap();
    fs::write(out.join("foreign"), b"unchanged").unwrap();
    let r = command(&out).output().unwrap();
    assert!(!r.status.success());
    assert_eq!(fs::read(out.join("foreign")).unwrap(), b"unchanged");
    assert!(!out.join("control").exists());
}
#[test]
fn cancellation_reaps_session_escape_before_ipc_cleanup_and_preserves_shared_replacement() {
    for sent in [Some(Signal::INT), Some(Signal::TERM), None] {
        let t = evidence();
        let records = t.as_path().join("records");
        let fixtures = t.as_path().join("fixtures");
        for kind in ["road", "qroad"] {
            fs::create_dir_all(fixtures.join(kind)).unwrap();
            fs::write(
                fixtures.join(kind).join("0000.capnp"),
                b"unused before fixture barrier",
            )
            .unwrap();
        }
        let out = t.as_path().join("comparison");
        let stderr = t.as_path().join("checker.stderr");
        let child = command(&out)
            .arg("--fixtures")
            .arg(&fixtures)
            .env("LOGGERD_LIFECYCLE_RECORDS", &records)
            .env(
                "LOGGERD_LIFECYCLE_MODE",
                if sent.is_none() { "error" } else { "hold" },
            )
            .stdout(Stdio::null())
            .stderr(File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
        let mut checker = Checker {
            child: Some(child),
            identity,
            peers: Vec::new(),
            stderr,
        };
        checker.wait("actual prebound receiver and escaped descendants", || {
            records.join("leaf.json").is_file() && records.join("paths.json").is_file()
        });
        let mut pids = Vec::new();
        for role in ["logger", "escaped", "leaf"] {
            let row: Value =
                serde_json::from_slice(&fs::read(records.join(format!("{role}.json"))).unwrap())
                    .unwrap();
            let pid = row["pid"].as_i64().unwrap() as i32;
            let id = pidfd_open(Pid::from_raw(pid).unwrap(), PidfdFlags::empty()).unwrap();
            checker.peers.push(id);
            pids.push(pid);
        }
        let paths: Value =
            serde_json::from_slice(&fs::read(records.join("paths.json")).unwrap()).unwrap();
        let namespace = PathBuf::from(paths["namespace"].as_str().unwrap());
        let socket = PathBuf::from(paths["socket"].as_str().unwrap());
        assert!(namespace.is_dir());
        assert!(fs::symlink_metadata(&socket)
            .unwrap()
            .file_type()
            .is_socket());
        fs::remove_file(&socket).unwrap();
        fs::write(&socket, b"foreign replacement").unwrap();
        if let Some(sent) = sent {
            pidfd_send_signal(&checker.identity, sent).unwrap();
        } else {
            fs::write(records.join("fail-now"), b"release actual worker error").unwrap();
        }
        let status = checker.finish();
        let expected = sent.map_or(1, |s| 128 + s.as_raw());
        assert_eq!(status.code(), Some(expected));
        for &pid in &pids {
            assert!(
                !Path::new(&format!("/proc/{pid}")).exists(),
                "unreaped descendant {pid}"
            );
        }
        assert!(!namespace.exists());
        let receipt: Value =
            serde_json::from_slice(&fs::read(out.join("control/cleanup.json")).unwrap()).unwrap();
        assert_eq!(receipt["processes_confirmed"], true);
        assert_eq!(receipt["error"], Value::Null);
        assert_eq!(receipt["pending_namespaces"], 0);
        assert_eq!(receipt["pending_receivers"], 0);
        let events = receipt["events"].as_array().unwrap();
        let cleanup = events
            .iter()
            .position(|v| {
                v["phase"] == "ipc-cleanup"
                    && v["resource"]["path"] == socket.to_string_lossy().as_ref()
            })
            .unwrap();
        let empty = events[..cleanup]
            .iter()
            .rposition(|v| v["phase"] == "adopted-children-empty")
            .unwrap();
        assert!(events[..empty]
            .iter()
            .any(|v| v["phase"] == "adopted-reaped"));
        assert_eq!(
            events[cleanup]["resource"]["status"],
            "preserved-replacement"
        );
        assert_eq!(fs::read(&socket).unwrap(), b"foreign replacement");
        fs::write(t.join("case.json"), serde_json::to_vec_pretty(&serde_json::json!({"expected_exit":expected,"actual_exit":status.code(),"descendants":pids,"cleanup_confirmed":true,"foreign_leaf_preserved":true})).unwrap()).unwrap();
        fs::remove_file(socket).unwrap();
    }
}
use std::os::unix::fs::FileTypeExt;
