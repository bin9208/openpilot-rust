//! Actual CLI ownership regression; no source/candidate policy responses supplied.
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fd::OwnedFd,
    fs::{open, Mode, OFlags},
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use serde_json::Value;
use std::{
    fs::{self, File},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixListener,
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
struct Identity {
    pid: Pid,
    fd: OwnedFd,
    exe: PathBuf,
}
impl Identity {
    fn exited(&self, timeout: Duration) -> bool {
        poll(
            &mut [PollFd::new(&self.fd, PollFlags::IN)],
            Some(&Timespec::try_from(timeout).unwrap()),
        )
        .unwrap()
            == 1
    }
}
struct OwnedProcesses(Vec<Identity>);
impl Drop for OwnedProcesses {
    fn drop(&mut self) {
        for identity in &self.0 {
            let _ = pidfd_send_signal(&identity.fd, Signal::KILL);
        }
    }
}
fn owned(token: &str) -> OwnedProcesses {
    let expected = format!("ENDPOINT_386_OWNER={token}").into_bytes();
    let mut found = Vec::new();
    for entry in fs::read_dir("/proc").unwrap().flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
            .and_then(Pid::from_raw)
        else {
            continue;
        };
        let matches = || {
            fs::read(entry.path().join("environ"))
                .is_ok_and(|env| env.split(|b| *b == 0).any(|part| part == expected))
        };
        if !matches() {
            continue;
        }
        let Ok(fd) = pidfd_open(pid, PidfdFlags::empty()) else {
            continue;
        };
        // Revalidate the unique token after obtaining the reserved identity.
        let Ok(exe) = fs::read_link(entry.path().join("exe")) else {
            continue;
        };
        if matches() {
            found.push(Identity { pid, fd, exe });
        }
    }
    OwnedProcesses(found)
}
struct Checker(Child, OwnedFd);
impl Drop for Checker {
    fn drop(&mut self) {
        let _ = pidfd_send_signal(&self.1, Signal::INT);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Foreign {
    path: PathBuf,
    _socket: UnixListener,
    identity: (u64, u64),
}
impl Drop for Foreign {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|meta| (meta.dev(), meta.ino()) == self.identity)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}
/// None binding selects Version collector; Some selects tombstone live runtime.
pub fn replacement_survives(
    repository: &Path,
    checker: &Path,
    collector: &Path,
    binding: Option<&Path>,
) {
    let home = tempfile::Builder::new()
        .prefix("endpoint-386-")
        .tempdir()
        .unwrap();
    let output = home.path().join("evidence");
    let marker = home.path().join("ack.json");
    let peer = home.path().join("blocked-native-peer");
    fs::write(&peer,b"#!/bin/sh\nprintf '{\"pid\":%s,\"prefix\":\"%s\"}\\n' \"$$\" \"$OPENPILOT_PREFIX\" > \"$ENDPOINT_386_MARKER\"\nexec /bin/sleep 30\n").unwrap();
    fs::set_permissions(&peer, fs::Permissions::from_mode(0o755)).unwrap();
    let token = home.path().file_name().unwrap().to_str().unwrap();
    let stderr = home.path().join("checker.stderr");
    let mut command = Command::new(checker);
    command.arg(&peer).arg(collector);
    if let Some(binding) = binding {
        command.arg(binding);
    }
    let child = command
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", repository)
        .env("ENDPOINT_386_OWNER", token)
        .env("ENDPOINT_386_MARKER", &marker)
        .stdin(Stdio::null())
        .stdout(File::create(home.path().join("checker.stdout")).unwrap())
        .stderr(File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let fd = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker(child, fd);
    let deadline = Instant::now() + Duration::from_secs(10);
    let acknowledgement: Value = loop {
        if let Some(value) = fs::read(&marker)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        {
            break value;
        }
        assert!(
            checker.0.try_wait().unwrap().is_none(),
            "checker failed before collector/peer ACK: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        assert!(Instant::now() < deadline, "peer never acknowledged");
        thread::sleep(Duration::from_millis(5));
    };
    let prefix = acknowledgement["prefix"].as_str().unwrap();
    assert!(prefix.starts_with(if binding.is_some() {
        "tombstone-live-"
    } else {
        "version-collector-"
    }));
    assert!(prefix
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    let endpoint = PathBuf::from(format!("/tmp/logmessage{prefix}"));
    // Reserve the filesystem inode, including after killing the collector. This
    // prevents inode recycling from disguising a distinct foreign replacement.
    let original = File::from(
        open(
            &endpoint,
            OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .unwrap(),
    );
    let metadata = original.metadata().unwrap();
    assert!(metadata.file_type().is_socket());
    let identities = owned(token);
    let peer_pid =
        Pid::from_raw(i32::try_from(acknowledgement["pid"].as_u64().unwrap()).unwrap()).unwrap();
    let peer = identities
        .0
        .iter()
        .find(|identity| identity.pid == peer_pid)
        .expect("ACK peer identity absent");
    assert!(!peer.exited(Duration::ZERO));
    let collector = collector.canonicalize().unwrap();
    let collectors = identities
        .0
        .iter()
        .filter(|identity| identity.exe == collector)
        .collect::<Vec<_>>();
    assert_eq!(collectors.len(), 1, "actual collector identity not unique");
    let collector = collectors[0];
    assert!(!collector.exited(Duration::ZERO));
    // Exclude ZeroMQ's own destructor from the unlink being tested. Only this
    // invocation's ready collector receives KILL; the blocked peer remains live.
    pidfd_send_signal(&collector.fd, Signal::KILL).unwrap();
    assert!(collector.exited(Duration::from_secs(2)));
    let current = fs::symlink_metadata(&endpoint).unwrap();
    assert_eq!(
        (current.dev(), current.ino()),
        (metadata.dev(), metadata.ino())
    );
    fs::remove_file(&endpoint).unwrap();
    let socket = UnixListener::bind(&endpoint).unwrap();
    let replacement = fs::symlink_metadata(&endpoint).unwrap();
    let foreign = Foreign {
        path: endpoint.clone(),
        _socket: socket,
        identity: (replacement.dev(), replacement.ino()),
    };
    assert_ne!(foreign.identity, (metadata.dev(), metadata.ino()));
    let started = Instant::now();
    pidfd_send_signal(&checker.1, Signal::INT).unwrap();
    let status = loop {
        if let Some(status) = checker.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "checker cancellation exceeded2s"
        );
        thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(status.signal(), Some(signal_hook::consts::SIGINT));
    assert!(
        peer.exited(Duration::from_secs(1)),
        "blocked owned peer survived"
    );
    assert!(!output.join("manifest.json").exists());
    let retained =
        fs::symlink_metadata(&endpoint).expect("outer cleanup deleted foreign replacement");
    assert_eq!((retained.dev(), retained.ino()), foreign.identity);
    assert!(!Path::new("/dev/shm")
        .join(format!("msgq_{prefix}"))
        .exists());
    drop(foreign);
    drop(identities);
    drop(original);
}
