//! Actual nested Probe/group cancellation with acknowledged identities.
#![cfg(feature = "managed-entry-native")]
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fd::OwnedFd,
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use std::{
    fs,
    os::unix::{
        fs::{FileTypeExt, MetadataExt},
        net::UnixListener,
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
struct ChildOwner(Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Checker {
    child: Child,
    identity: OwnedFd,
}
impl Drop for Checker {
    fn drop(&mut self) {
        // Give the real outer owner its cancellation path before the fallback
        // kills token-bound identities. No numeric group from a receipt is used.
        let _ = pidfd_send_signal(&self.identity, Signal::INT);
        let deadline = Instant::now() + Duration::from_secs(2);
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
struct Identity(OwnedFd);
impl Identity {
    fn exited(&self) -> bool {
        poll(
            &mut [PollFd::new(&self.0, PollFlags::IN)],
            Some(&Timespec::default()),
        )
        .unwrap()
            != 0
    }
}
impl Drop for Identity {
    fn drop(&mut self) {
        let _ = pidfd_send_signal(&self.0, Signal::KILL);
    }
}
struct TaggedCleanup(Vec<u8>);
impl Drop for TaggedCleanup {
    fn drop(&mut self) {
        let Ok(entries) = fs::read_dir("/proc") else {
            return;
        };
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<i32>().ok())
                .and_then(Pid::from_raw)
            else {
                continue;
            };
            let Ok(identity) = pidfd_open(pid, PidfdFlags::empty()) else {
                continue;
            };
            let env = fs::read(entry.path().join("environ")).unwrap_or_default();
            if env.split(|b| *b == 0).any(|value| value == self.0) {
                let _ = pidfd_send_signal(&identity, Signal::KILL);
            }
        }
    }
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn artifact(name: &str) -> PathBuf {
    let path = fs::canonicalize(
        std::env::var_os(name)
            .unwrap_or_else(|| panic!("{name} must name the verified actual artifact")),
    )
    .unwrap();
    assert!(path.is_file());
    path
}
fn group(pid: i32) -> (i32, i32) {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let fields = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    (fields[1].parse().unwrap(), fields[2].parse().unwrap())
}
fn check_owned_cancellation(replace_endpoint: bool) {
    let temp = tempfile::tempdir().unwrap();
    let ack = temp.path().join("ack");
    let output = temp.path().join("evidence");
    let token = format!(
        "managed-ownership-{}",
        temp.path().file_name().unwrap().to_string_lossy()
    );
    let token_env = format!("MANAGED_OWNERSHIP_TOKEN={token}").into_bytes();
    let _fallback = TaggedCleanup(token_env.clone());
    let helper = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/managed-entry/blocked-child")
        .canonicalize()
        .unwrap();
    let mut foreign = ChildOwner(
        Command::new("/bin/sleep")
            .arg("60")
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    let stderr = temp.path().join("checker.stderr");
    let child = Command::new(env!("CARGO_BIN_EXE_check-managed-entry"))
        .arg(helper)
        .arg(artifact("MANAGED_COLLECTOR_BINARY"))
        .arg(artifact("MANAGED_PARAMS_BINDING"))
        .arg(&output)
        .args(["--case", "normal"])
        .env("OPENPILOT_RUST_ROOT", root())
        .env("MANAGED_OWNERSHIP_TOKEN", token)
        .env("MANAGED_OWNERSHIP_ACK", &ack)
        .stdin(Stdio::null())
        .stdout(fs::File::create(temp.path().join("checker.stdout")).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .process_group(0)
        .spawn()
        .unwrap();
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker { child, identity };
    let startup = Instant::now() + Duration::from_secs(40);
    let (ids, pids) = loop {
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "checker exited before candidate acknowledgement: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        if let Some(pids) = fs::read_to_string(&ack)
            .ok()
            .and_then(|text| {
                text.split_whitespace()
                    .map(str::parse::<i32>)
                    .collect::<Result<Vec<_>, _>>()
                    .ok()
            })
            .filter(|pids| pids.len() == 2)
        {
            let ids = pids
                .iter()
                .map(|pid| {
                    let id = pidfd_open(Pid::from_raw(*pid).unwrap(), PidfdFlags::empty()).ok()?;
                    let env = fs::read(format!("/proc/{pid}/environ")).ok()?;
                    env.split(|b| *b == 0)
                        .any(|entry| entry == token_env)
                        .then_some(Identity(id))
                })
                .collect::<Option<Vec<_>>>();
            if let Some(ids) = ids {
                break (ids, pids);
            }
        }
        assert!(
            Instant::now() < startup,
            "candidate never acknowledged; stderr={}",
            fs::read_to_string(&stderr).unwrap()
        );
        thread::sleep(Duration::from_millis(5));
    };
    let (worker_pid, probe_group) = group(pids[0]);
    let (_, grandchild_group) = group(pids[1]);
    // A different group here would escape the actual outer worker's cleanup.
    assert_eq!(
        probe_group, worker_pid,
        "nested Probe created a separate group"
    );
    assert_eq!(grandchild_group, probe_group);
    assert_ne!(probe_group, checker.child.id() as i32);
    assert!(ids.iter().all(|id| !id.exited()));
    let replacement = if replace_endpoint {
        let home = output.join("original-collector/normal/native/collector/home");
        let roots = fs::read_dir(&home)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(roots.len(), 1);
        let name = roots[0].file_name().unwrap().to_str().unwrap();
        let prefix = name.strip_prefix(".comma").unwrap();
        let endpoint = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        assert!(fs::symlink_metadata(&endpoint)
            .unwrap()
            .file_type()
            .is_socket());
        fs::remove_file(&endpoint).unwrap();
        let listener = UnixListener::bind(&endpoint).unwrap();
        let metadata = fs::symlink_metadata(&endpoint).unwrap();
        Some(ForeignEndpoint {
            path: endpoint,
            identity: (metadata.dev(), metadata.ino()),
            _listener: listener,
        })
    } else {
        None
    };
    let stopped = Instant::now();
    pidfd_send_signal(&checker.identity, Signal::INT).unwrap();
    loop {
        if (poll(
            &mut [PollFd::new(&checker.identity, PollFlags::IN)],
            Some(&Timespec::default()),
        )
        .unwrap()
            != 0)
            && ids.iter().all(Identity::exited)
        {
            break;
        }
        assert!(
            stopped.elapsed() < Duration::from_secs(2),
            "checker, Probe or grandchild survived cancellation"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!checker.child.wait().unwrap().success());
    assert!(!output.join("manifest.json").exists());
    if let Some(endpoint) = &replacement {
        let identity = fs::symlink_metadata(&endpoint.path)
            .ok()
            .map(|m| (m.dev(), m.ino()));
        assert_eq!(
            identity,
            Some(endpoint.identity),
            "#386 cleanup deleted a foreign replacement endpoint before first observation"
        );
    }
    assert!(
        foreign.0.try_wait().unwrap().is_none(),
        "unrelated child was signalled"
    );
}

struct ForeignEndpoint {
    path: PathBuf,
    identity: (u64, u64),
    _listener: UnixListener,
}
impl Drop for ForeignEndpoint {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .ok()
            .map(|m| (m.dev(), m.ino()))
            == Some(self.identity)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}
#[test]
fn checker_sigint_cleans_acknowledged_same_group_probe_and_grandchild_without_foreign_collateral() {
    check_owned_cancellation(false);
}
#[test]
fn endpoint_replaced_after_live_acknowledgement_survives_checker_cancellation() {
    check_owned_cancellation(true);
}
