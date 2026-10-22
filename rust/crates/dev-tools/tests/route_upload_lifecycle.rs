#![cfg(target_os = "linux")]
//! Failure mechanics, kept separate from the real original/native daemon proof.
use rustix::process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal};
use serde_json::Value;
use std::{
    fs::{self, File},
    os::{
        fd::OwnedFd,
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::{symlink, MetadataExt},
            net::UnixListener,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};
const FIXTURE: &str = env!("CARGO_BIN_EXE_route-upload-lifecycle-fixture");
struct Checker {
    child: Option<Child>,
    identity: OwnedFd,
    stderr: PathBuf,
    peers: Vec<OwnedFd>,
}
impl Drop for Checker {
    fn drop(&mut self) {
        for identity in &self.peers {
            let _ = pidfd_send_signal(identity, Signal::KILL);
        }
        if let Some(child) = &mut self.child {
            let _ = pidfd_send_signal(&self.identity, Signal::TERM);
            let deadline = Instant::now() + Duration::from_secs(20);
            while Instant::now() < deadline {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
            let _ = pidfd_send_signal(&self.identity, Signal::KILL);
            let _ = child.wait();
        }
    }
}
impl Checker {
    fn start(
        manager: bool,
        binaries: &Path,
        probe: &Path,
        output: &Path,
        records: &Path,
        mode: Option<&str>,
    ) -> Self {
        let stderr = output.with_extension("stderr");
        let mut command = Command::new(if manager {
            env!("CARGO_BIN_EXE_check-manager-log-upload")
        } else {
            env!("CARGO_BIN_EXE_check-runtime-log-upload")
        });
        command
            .arg("--binaries")
            .arg(binaries)
            .arg("--output")
            .arg(output);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        command.arg("--root").arg(root);
        if manager {
            command.arg("--probe").arg(probe);
        }
        if let Some(mode) = mode {
            command
                .env("ROUTE_UPLOAD_LIFECYCLE_FIXTURE", mode)
                .env("ROUTE_UPLOAD_FIXTURE_RECORDS", records);
        } else {
            command
                .env_remove("ROUTE_UPLOAD_LIFECYCLE_FIXTURE")
                .env_remove("ROUTE_UPLOAD_FIXTURE_RECORDS");
        }
        let child = command
            .stdout(Stdio::null())
            .stderr(Stdio::from(File::create(&stderr).unwrap()))
            .spawn()
            .unwrap();
        let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
        Self {
            child: Some(child),
            identity,
            stderr,
            peers: Vec::new(),
        }
    }
    fn wait_for(&mut self, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !condition() {
            assert!(
                self.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                "checker exited before fixture readiness: {}",
                fs::read_to_string(&self.stderr).unwrap()
            );
            assert!(
                Instant::now() < deadline,
                "fixture readiness exceeded bound: {}",
                fs::read_to_string(&self.stderr).unwrap()
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                self.child = None;
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "checker failed bounded completion: {}",
                fs::read_to_string(&self.stderr).unwrap()
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn remember_peer(&mut self, pid: i32, records: &Path) {
        let expected = [
            b"ROUTE_UPLOAD_FIXTURE_RECORDS=".as_slice(),
            records.as_os_str().as_bytes(),
        ]
        .concat();
        let owns = || {
            fs::read(format!("/proc/{pid}/environ")).is_ok_and(|bytes| {
                bytes
                    .split(|byte| *byte == 0)
                    .any(|entry| entry == expected)
            })
        };
        assert!(
            owns(),
            "fixture PID did not inherit its exact owned records root"
        );
        let identity = pidfd_open(Pid::from_raw(pid).unwrap(), PidfdFlags::empty()).unwrap();
        assert!(
            owns(),
            "fixture process identity changed while opening pidfd"
        );
        self.peers.push(identity);
    }
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn owned(output: &Path, path: &Path) -> bool {
    fs::read_to_string(output.join("ownership.jsonl")).is_ok_and(|text| {
        text.lines().any(|line| {
            serde_json::from_str::<Value>(line).is_ok_and(|row| {
                row["path"].as_array().is_some_and(|bytes| {
                    bytes
                        .iter()
                        .filter_map(Value::as_u64)
                        .map(|byte| byte as u8)
                        .collect::<Vec<_>>()
                        == path.as_os_str().as_bytes()
                })
            })
        })
    })
}
fn partial_collector(signal: Option<Signal>, replace: bool) {
    let temporary = tempfile::tempdir().unwrap();
    let binaries = temporary.path().join("bin");
    fs::create_dir(&binaries).unwrap();
    symlink(FIXTURE, binaries.join("openpilot-logmessaged")).unwrap();
    let output = temporary.path().join("output");
    let records = temporary.path().join("records");
    let mode = if signal.is_some() {
        "collector-partial"
    } else {
        "collector-partial-exit"
    };
    let mut checker = Checker::start(
        false,
        &binaries,
        Path::new(FIXTURE),
        &output,
        &records,
        Some(mode),
    );
    checker.wait_for(|| records.join("paths.json").is_file());
    let paths = read(&records.join("paths.json"));
    let socket = PathBuf::from(paths["socket"].as_str().unwrap());
    let namespace = PathBuf::from(paths["namespace"].as_str().unwrap());
    let ipc_root = PathBuf::from(paths["ipc_root"].as_str().unwrap());
    if signal.is_some() {
        checker.wait_for(|| owned(&output, &socket));
    }
    let moved = temporary.path().join("old-socket");
    let foreign = if replace {
        fs::rename(&socket, &moved).unwrap();
        Some(UnixListener::bind(&socket).unwrap())
    } else {
        None
    };
    if let Some(signal) = signal {
        pidfd_send_signal(&checker.identity, signal).unwrap();
    }
    let status = checker.finish();
    assert_eq!(
        status.code(),
        Some(signal.map_or(1, |signal| 128 + signal.as_raw()))
    );
    assert!(!output.join("result.json").exists());
    assert!(!namespace.exists(), "owned namespace leaked");
    if !replace {
        assert!(!ipc_root.exists(), "preowned bind namespace leaked");
    }
    if replace {
        assert!(socket.exists(), "foreign replacement endpoint was removed");
        drop(foreign);
        fs::remove_file(&socket).unwrap();
        fs::remove_file(moved).unwrap();
        fs::remove_dir(&ipc_root).unwrap();
    } else {
        assert!(!socket.exists(), "partial-readiness socket leaked");
    }
}
#[test]
fn partial_collector_socket_is_owned_before_msgq_readiness_on_cancel_and_exit() {
    partial_collector(Some(Signal::INT), false);
    partial_collector(Some(Signal::TERM), false);
    partial_collector(None, false);
}
#[test]
fn cancellation_preserves_foreign_replacement_of_the_held_socket() {
    partial_collector(Some(Signal::INT), true);
}
#[test]
fn manager_cancellation_reaps_adopted_setsid_descendants_and_owned_ipc() {
    for signal in [Signal::INT, Signal::TERM] {
        let temporary = tempfile::tempdir().unwrap();
        let binaries = temporary.path().join("bin");
        fs::create_dir(&binaries).unwrap();
        let output = temporary.path().join("output");
        let records = temporary.path().join("records");
        let mut checker = Checker::start(
            true,
            &binaries,
            Path::new(FIXTURE),
            &output,
            &records,
            Some("probe-escape"),
        );
        checker.wait_for(|| {
            records.join("leaf.json").is_file()
                && records.join("escaped.json").is_file()
                && records.join("paths.json").is_file()
        });
        let paths = read(&records.join("paths.json"));
        let socket = PathBuf::from(paths["socket"].as_str().unwrap());
        let namespace = PathBuf::from(paths["namespace"].as_str().unwrap());
        let stats = PathBuf::from(paths["stats"].as_str().unwrap());
        let ipc_root = PathBuf::from(paths["ipc_root"].as_str().unwrap());
        checker.wait_for(|| {
            owned(&output, &socket) && owned(&output, &namespace) && owned(&output, &stats)
        });
        let mut pids = Vec::new();
        for role in ["probe", "escaped", "leaf"] {
            let record = read(&records.join(format!("{role}.json")));
            let pid = record["pid"].as_i64().unwrap() as i32;
            checker.remember_peer(pid, &records);
            pids.push(pid);
        }
        assert_ne!(
            read(&records.join("probe.json"))["session"],
            read(&records.join("escaped.json"))["session"],
            "fixture did not escape its probe session"
        );
        pidfd_send_signal(&checker.identity, signal).unwrap();
        assert_eq!(checker.finish().code(), Some(128 + signal.as_raw()));
        for pid in pids {
            assert!(
                !PathBuf::from(format!("/proc/{pid}")).exists(),
                "owned adopted process was not reaped: {pid}"
            );
        }
        assert!(!socket.exists() && !namespace.exists() && !stats.exists() && !ipc_root.exists());
        assert!(!output.join("result.json").exists());
    }
}
#[test]
#[ignore = "hosted actual collector prerequisite; set ROUTE_UPLOAD_LIVE_BINARIES"]
fn actual_collector_cancelled_while_logger_waits_cleans_its_real_ipc() {
    let actual = PathBuf::from(
        std::env::var_os("ROUTE_UPLOAD_LIVE_BINARIES")
            .expect("actual native prerequisites required"),
    );
    for (signal, replace) in [
        (Signal::INT, false),
        (Signal::TERM, false),
        (Signal::INT, true),
        (Signal::TERM, true),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let binaries = temporary.path().join("bin");
        fs::create_dir(&binaries).unwrap();
        symlink(
            actual.join("openpilot-logmessaged"),
            binaries.join("openpilot-logmessaged"),
        )
        .unwrap();
        symlink(FIXTURE, binaries.join("openpilot-loggerd")).unwrap();
        let output = temporary.path().join("output");
        let records = temporary.path().join("records");
        let mut checker = Checker::start(
            false,
            &binaries,
            Path::new(FIXTURE),
            &output,
            &records,
            Some("logger-sleep"),
        );
        checker.wait_for(|| records.join("logger.json").is_file());
        let ownership = fs::read_to_string(output.join("ownership.jsonl")).unwrap();
        let paths = ownership
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .map(|row| {
                PathBuf::from(std::ffi::OsString::from_vec(
                    row["path"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|value| value.as_u64().unwrap() as u8)
                        .collect(),
                ))
            })
            .collect::<Vec<_>>();
        let endpoint = paths
            .iter()
            .find(|path| path.file_name() == Some(std::ffi::OsStr::new("logmessage")))
            .expect("actual collector endpoint must be acknowledged")
            .clone();
        let ipc_root = endpoint.parent().unwrap().to_path_buf();
        let moved = temporary.path().join("original-collector-socket");
        let replacement = if replace {
            fs::rename(&endpoint, &moved).unwrap();
            let listener = UnixListener::bind(&endpoint).unwrap();
            let identity = fs::symlink_metadata(&endpoint).unwrap();
            Some((listener, identity))
        } else {
            None
        };
        pidfd_send_signal(&checker.identity, signal).unwrap();
        assert_eq!(checker.finish().code(), Some(128 + signal.as_raw()));
        for path in paths {
            if replace && (path == endpoint || path == ipc_root) {
                continue;
            }
            assert!(
                !path.exists(),
                "real collector resource leaked: {}",
                path.display()
            );
        }
        if let Some((listener, expected)) = replacement {
            let actual = fs::symlink_metadata(&endpoint)
                .expect("actual collector cancellation removed foreign leaf");
            assert_eq!(
                (actual.dev(), actual.ino()),
                (expected.dev(), expected.ino())
            );
            assert!(
                ipc_root.is_dir(),
                "ancestor cleanup removed the foreign socket"
            );
            drop(listener);
            fs::remove_file(&endpoint).unwrap();
            fs::remove_dir(&ipc_root).unwrap();
            fs::remove_file(moved).unwrap();
        }
        assert!(!output.join("result.json").exists());
    }
}

#[test]
fn manager_namespace_is_owned_before_probe_params_exist() {
    for signal in [Signal::INT, Signal::TERM] {
        let temporary = tempfile::tempdir().unwrap();
        let binaries = temporary.path().join("bin");
        fs::create_dir(&binaries).unwrap();
        let output = temporary.path().join("output");
        let records = temporary.path().join("records");
        let mut checker = Checker::start(
            true,
            &binaries,
            Path::new(FIXTURE),
            &output,
            &records,
            Some("probe-early-namespace"),
        );
        checker.wait_for(|| records.join("paths.json").is_file());
        let paths = read(&records.join("paths.json"));
        let namespace = PathBuf::from(paths["namespace"].as_str().unwrap());
        assert!(owned(&output, &namespace));
        assert!(!output.join("manager/params").exists());
        pidfd_send_signal(&checker.identity, signal).unwrap();
        assert_eq!(checker.finish().code(), Some(128 + signal.as_raw()));
        assert!(!namespace.exists());
        assert!(!output.join("result.json").exists());
    }
}
#[test]
fn foreign_global_stats_socket_is_outside_the_native_owned_namespace() {
    assert!(!Path::new("/tmp/stats").exists());
    let temporary = tempfile::tempdir().unwrap();
    let binaries = temporary.path().join("bin");
    fs::create_dir(&binaries).unwrap();
    let output = temporary.path().join("output");
    let records = temporary.path().join("records");
    let mut checker = Checker::start(
        true,
        &binaries,
        Path::new(FIXTURE),
        &output,
        &records,
        Some("probe-early-namespace"),
    );
    checker.wait_for(|| records.join("paths.json").is_file());
    let foreign = UnixListener::bind("/tmp/stats").unwrap();
    thread::sleep(Duration::from_millis(80));
    assert!(!owned(&output, Path::new("/tmp/stats")));
    pidfd_send_signal(&checker.identity, Signal::TERM).unwrap();
    assert_eq!(checker.finish().code(), Some(143));
    assert!(Path::new("/tmp/stats").exists());
    drop(foreign);
    fs::remove_file("/tmp/stats").unwrap();
}

fn live_descendants(root: i32) -> Vec<(i32, OwnedFd, PathBuf)> {
    let mut found = Vec::new();
    let mut queue = vec![root];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(owner) = queue.pop() {
        let Ok(tasks) = fs::read_dir(format!("/proc/{owner}/task")) else {
            continue;
        };
        for task in tasks.flatten() {
            let Ok(children) = fs::read_to_string(task.path().join("children")) else {
                continue;
            };
            for pid in children
                .split_whitespace()
                .filter_map(|pid| pid.parse::<i32>().ok())
            {
                if !seen.insert(pid) {
                    continue;
                }
                let parent = || {
                    fs::read_to_string(format!("/proc/{pid}/status"))
                        .ok()
                        .and_then(|status| {
                            status
                                .lines()
                                .find_map(|line| line.strip_prefix("PPid:\t"))
                                .and_then(|p| p.trim().parse::<i32>().ok())
                        })
                };
                if parent() != Some(owner) {
                    continue;
                }
                let Ok(identity) = pidfd_open(Pid::from_raw(pid).unwrap(), PidfdFlags::empty())
                else {
                    continue;
                };
                if parent() != Some(owner) {
                    continue;
                }
                let Ok(exe) = fs::read_link(format!("/proc/{pid}/exe")) else {
                    continue;
                };
                found.push((pid, identity, exe));
                queue.push(pid);
            }
        }
    }
    found
}
#[test]
#[ignore = "hosted actual manager prerequisite; set ROUTE_UPLOAD_LIVE_BINARIES and ROUTE_UPLOAD_LIVE_PROBE"]
fn actual_manager_cancelled_with_running_native_children_reaps_tree_and_owned_ipc() {
    let binaries = PathBuf::from(
        std::env::var_os("ROUTE_UPLOAD_LIVE_BINARIES").expect("actual binaries required"),
    );
    let probe =
        PathBuf::from(std::env::var_os("ROUTE_UPLOAD_LIVE_PROBE").expect("actual probe required"));
    for signal in [Signal::INT, Signal::TERM] {
        assert!(!Path::new("/tmp/stats").exists());
        let foreign = UnixListener::bind("/tmp/stats").unwrap();
        let foreign_identity = fs::symlink_metadata("/tmp/stats").unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("output");
        let mut checker = Checker::start(
            true,
            &binaries,
            &probe,
            &output,
            &temporary.path().join("records"),
            None,
        );
        let checker_pid = checker.child.as_ref().unwrap().id() as i32;
        let mut peers = Vec::new();
        checker.wait_for(|| {
            let has_stats = fs::read_to_string(output.join("ownership.jsonl")).is_ok_and(|text| {
                text.lines().any(|line| {
                    serde_json::from_str::<Value>(line).is_ok_and(|row| {
                        row["path"].as_array().is_some_and(|bytes| {
                            bytes
                                .iter()
                                .filter_map(Value::as_u64)
                                .map(|b| b as u8)
                                .collect::<Vec<_>>()
                                .ends_with(b"/stats")
                        })
                    })
                })
            });
            if !has_stats {
                return false;
            }
            peers = live_descendants(checker_pid);
            ["openpilot-manager", "openpilot-logmessaged", "statsd-rs"]
                .iter()
                .all(|name| {
                    peers
                        .iter()
                        .any(|(_, _, exe)| exe.file_name() == Some(std::ffi::OsStr::new(name)))
                })
        });
        let paths = fs::read_to_string(output.join("ownership.jsonl"))
            .unwrap()
            .lines()
            .map(|line| {
                let row: Value = serde_json::from_str(line).unwrap();
                PathBuf::from(std::ffi::OsString::from_vec(
                    row["path"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u8)
                        .collect(),
                ))
            })
            .collect::<Vec<_>>();
        assert!(paths
            .iter()
            .any(|path| path.file_name() == Some(std::ffi::OsStr::new("logmessage"))));
        let pids = peers.iter().map(|(pid, _, _)| *pid).collect::<Vec<_>>();
        checker.peers.extend(peers.into_iter().map(|(_, fd, _)| fd));
        pidfd_send_signal(&checker.identity, signal).unwrap();
        assert_eq!(checker.finish().code(), Some(128 + signal.as_raw()));
        for pid in pids {
            assert!(
                !Path::new(&format!("/proc/{pid}")).exists(),
                "actual native child not reaped: {pid}"
            );
        }
        for path in paths {
            assert!(
                !path.exists(),
                "actual owned IPC leaked: {}",
                path.display()
            );
        }
        assert!(!output.join("result.json").exists());
        let actual = fs::symlink_metadata("/tmp/stats").unwrap();
        assert_eq!(
            (actual.dev(), actual.ino()),
            (foreign_identity.dev(), foreign_identity.ino())
        );
        drop(foreign);
        fs::remove_file("/tmp/stats").unwrap();
    }
}

fn socket_bindings(output: &Path, path: &Path) -> std::collections::BTreeSet<(u64, u64)> {
    fs::read_to_string(output.join("ownership.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let row: Value = serde_json::from_str(line).ok()?;
            let name = row["path"]
                .as_array()?
                .iter()
                .filter_map(Value::as_u64)
                .map(|b| b as u8)
                .collect::<Vec<_>>();
            (name == path.as_os_str().as_bytes())
                .then(|| (row["dev"].as_u64().unwrap(), row["ino"].as_u64().unwrap()))
        })
        .collect()
}
#[test]
fn restarted_stats_socket_is_reauthenticated_and_the_latest_inode_is_cleaned() {
    for signal in [Signal::INT, Signal::TERM] {
        let temporary = tempfile::tempdir().unwrap();
        let binaries = temporary.path().join("bin");
        fs::create_dir(&binaries).unwrap();
        let output = temporary.path().join("output");
        let records = temporary.path().join("records");
        let mut checker = Checker::start(
            true,
            &binaries,
            Path::new(FIXTURE),
            &output,
            &records,
            Some("probe-rebind"),
        );
        checker.wait_for(|| records.join("rebound").is_file());
        let paths = read(&records.join("paths.json"));
        let stats = PathBuf::from(paths["stats"].as_str().unwrap());
        let ipc_root = PathBuf::from(paths["ipc_root"].as_str().unwrap());
        checker.wait_for(|| socket_bindings(&output, &stats).len() == 2);
        pidfd_send_signal(&checker.identity, signal).unwrap();
        assert_eq!(checker.finish().code(), Some(128 + signal.as_raw()));
        assert!(
            !stats.exists() && !ipc_root.exists(),
            "restarted socket or owned namespace leaked"
        );
        assert!(!output.join("result.json").exists());
    }
}
#[test]
#[ignore = "hosted actual collector prerequisite; set ROUTE_UPLOAD_LIVE_BINARIES"]
fn actual_collector_bind_then_immediate_initialization_error_cleans_preowned_ipc() {
    let actual = PathBuf::from(
        std::env::var_os("ROUTE_UPLOAD_LIVE_BINARIES").expect("actual binaries required"),
    );
    assert!(actual.join("openpilot-logmessaged").is_file());
    let temporary = tempfile::tempdir().unwrap();
    let binaries = temporary.path().join("bin");
    fs::create_dir(&binaries).unwrap();
    for name in ["openpilot-logmessaged", "openpilot-loggerd"] {
        symlink(FIXTURE, binaries.join(name)).unwrap();
    }
    let output = temporary.path().join("output");
    let records = temporary.path().join("records");
    let mut checker = Checker::start(
        false,
        &binaries,
        Path::new(FIXTURE),
        &output,
        &records,
        Some("collector-actual-exit"),
    );
    let status = checker.finish();
    assert_eq!(status.code(), Some(1));
    let paths = read(&records.join("paths.json"));
    for key in ["namespace", "socket", "ipc_root"] {
        assert!(
            !Path::new(paths[key].as_str().unwrap()).exists(),
            "actual collector bind-then-exit leaked {key}"
        );
    }
    let error = fs::read_to_string(output.join("openpilot-logmessaged.log")).unwrap();
    assert!(
        error.contains("existing msgq queue has incompatible size or type"),
        "actual post-bind messaging initialization error missing: {error}"
    );
    assert!(!output.join("result.json").exists());
}

#[test]
fn cancellation_preserves_a_foreign_replacement_of_the_preowned_ipc_root() {
    let temporary = tempfile::tempdir().unwrap();
    let binaries = temporary.path().join("bin");
    fs::create_dir(&binaries).unwrap();
    symlink(FIXTURE, binaries.join("openpilot-logmessaged")).unwrap();
    let output = temporary.path().join("output");
    let records = temporary.path().join("records");
    let mut checker = Checker::start(
        false,
        &binaries,
        Path::new(FIXTURE),
        &output,
        &records,
        Some("collector-partial"),
    );
    checker.wait_for(|| records.join("paths.json").is_file());
    let paths = read(&records.join("paths.json"));
    let ipc_root = PathBuf::from(paths["ipc_root"].as_str().unwrap());
    let namespace = PathBuf::from(paths["namespace"].as_str().unwrap());
    let moved = temporary.path().join("original-ipc");
    fs::rename(&ipc_root, &moved).unwrap();
    fs::create_dir(&ipc_root).unwrap();
    fs::write(ipc_root.join("foreign"), b"keep foreign namespace\n").unwrap();
    let foreign = UnixListener::bind(ipc_root.join("logmessage")).unwrap();
    let _ = pidfd_send_signal(&checker.identity, Signal::TERM);
    assert!(
        [Some(1), Some(143)].contains(&checker.finish().code()),
        "namespace replacement must fail or report the delivered cancellation"
    );
    assert!(!namespace.exists());
    assert_eq!(
        fs::read(ipc_root.join("foreign")).unwrap(),
        b"keep foreign namespace\n"
    );
    assert!(ipc_root.join("logmessage").exists());
    assert!(!output.join("result.json").exists());
    drop(foreign);
    fs::remove_dir_all(&ipc_root).unwrap();
    fs::remove_dir_all(moved).unwrap();
}
