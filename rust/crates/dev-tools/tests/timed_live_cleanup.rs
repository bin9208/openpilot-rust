//! Public live controller cleanup on failed startup and worker cancellation.
#![cfg(all(target_os = "linux", feature = "timed-live-native"))]

use openpilot_dev_tools::{timed_check::Options, timed_live::invoke};
use rustix::{
    fd::OwnedFd,
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

fn options(output: PathBuf) -> Options {
    Options {
        root: Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../..")).into(),
        binary: "/unused-before-date-preflight".into(),
        output,
        reference: env!("CARGO_BIN_EXE_timed-source-reference").into(),
        command: env!("CARGO_BIN_EXE_timed-command-fixture").into(),
        worker: vec![],
    }
}

#[test]
fn failed_worker_spawn_does_not_consume_previous_invocations_ownership() {
    let directory = tempfile::tempdir().unwrap();
    // This guard alone owns this fresh namespace, never the attempted worker.
    let namespace = tempfile::Builder::new()
        .prefix("msgq_timed_cleanup_")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = namespace
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let sentinel = namespace.path().join("unrelated-owner-sentinel");
    fs::write(&sentinel, b"owned by the test, not the missing worker").unwrap();
    let options = options(directory.path().join("reused-output"));
    fs::create_dir(&options.output).unwrap();
    fs::write(
        options.output.join("ownership.json"),
        serde_json::to_vec(&json!({
            "prefix":prefix,
            "worker_pid":std::process::id(),
            "owns_namespace":true,
            "owns_endpoint":false
        }))
        .unwrap(),
    )
    .unwrap();
    let result = invoke(
        &options,
        &directory.path().join("nonexistent-worker"),
        &json!({"kind":"lifecycle"}),
    );
    assert!(result.is_err(), "a nonexistent worker unexpectedly ran");
    assert_eq!(
        fs::read(&sentinel).ok().as_deref(),
        Some(b"owned by the test, not the missing worker".as_slice()),
        "failed startup deleted a namespace described only by an old receipt"
    );
}

fn quoted(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}
fn script(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn marked_pids(path: &Path) -> Vec<u32> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|text| text.parse().ok())
        .collect()
}
fn running(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(") ")
                .map(|(_, fields)| !fields.starts_with('Z'))
        })
        .unwrap_or(false)
}
fn wait_for(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while !condition() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
    }
    true
}

// Keep process identities, not reusable numeric process groups, for test cleanup.
// The fixture sleeps are also bounded if a failure occurs before marker capture.
struct Cancellation {
    worker_marker: PathBuf,
    nested_marker: PathBuf,
    handles: Vec<(u32, OwnedFd)>,
    thread: Option<JoinHandle<()>>,
}
impl Cancellation {
    fn remember(&mut self, path: &Path) {
        for pid in marked_pids(path) {
            if self.handles.iter().any(|(known, _)| *known == pid) {
                continue;
            }
            let Some(pid_value) = Pid::from_raw(pid as i32) else {
                continue;
            };
            let Ok(fd) = pidfd_open(pid_value, PidfdFlags::empty()) else {
                continue;
            };
            self.handles.push((pid, fd));
        }
    }
}
impl Drop for Cancellation {
    fn drop(&mut self) {
        self.remember(&self.worker_marker.clone());
        self.remember(&self.nested_marker.clone());
        for (_, fd) in &self.handles {
            let _ = pidfd_send_signal(fd, Signal::KILL);
        }
        // Pick up a marker published concurrently with cancellation as well.
        thread::sleep(Duration::from_millis(20));
        self.remember(&self.nested_marker.clone());
        for (_, fd) in &self.handles {
            let _ = pidfd_send_signal(fd, Signal::KILL);
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

fn cancellation_during_nested_call(lifecycle: bool) {
    let directory = tempfile::tempdir().unwrap();
    let worker_marker = directory.path().join("worker.pid");
    let nested_marker = directory.path().join("nested.pids");
    let worker = directory.path().join("worker.sh");
    script(
        &worker,
        &format!(
            "printf '%s\\n' \"$$\" > {}\nexec {} \"$@\"",
            quoted(&worker_marker),
            quoted(Path::new(env!("CARGO_BIN_EXE_timed-runtime-worker")))
        ),
    );
    let nested = directory.path().join("nested.sh");
    let block = format!(
        "/bin/sleep 15 &\nchild=$!\nprintf '%s %s\\n' \"$$\" \"$child\" > {}\nwait \"$child\"",
        quoted(&nested_marker)
    );
    let mut options = options(directory.path().join("output"));
    let case = if lifecycle {
        let entry = std::env::var_os("TIMED_NATIVE_ENTRY")
            .expect("set TIMED_NATIVE_ENTRY to the current-source openpilot-timed ELF");
        assert!(fs::read(&entry).unwrap().starts_with(b"\x7fELF"));
        script(
            &nested,
            &format!(
                "if [ \"$1\" = --help ]; then\n{block}\nelse\nexec {} \"$@\"\nfi",
                quoted(Path::new(&entry))
            ),
        );
        options.binary = nested;
        json!({"kind":"lifecycle"})
    } else {
        script(&nested, &block);
        options.reference = nested;
        json!({"kind":"date-daemon","zone":"UTC","milliseconds":1790000000000i64,"implementation":"rust"})
    };
    let (sender, receiver) = mpsc::channel();
    let mut owned = Cancellation {
        worker_marker: worker_marker.clone(),
        nested_marker: nested_marker.clone(),
        handles: vec![],
        thread: None,
    };
    owned.thread = Some(thread::spawn(move || {
        let result = invoke(&options, &worker, &case).map_err(|error| error.to_string());
        let _ = sender.send(result);
    }));
    assert!(
        wait_for(Duration::from_secs(12), || {
            marked_pids(&worker_marker).len() == 1 && marked_pids(&nested_marker).len() == 2
        }),
        "actual controller did not reach the nested call: {:?}",
        receiver.try_recv()
    );
    owned.remember(&worker_marker);
    owned.remember(&nested_marker);
    let worker_pid = marked_pids(&worker_marker)[0];
    let nested_pids = marked_pids(&nested_marker);
    assert!(nested_pids.iter().all(|pid| running(*pid)));
    let worker_fd = &owned
        .handles
        .iter()
        .find(|(pid, _)| *pid == worker_pid)
        .unwrap()
        .1;
    pidfd_send_signal(worker_fd, Signal::KILL).unwrap();
    let result = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("outer controller did not finish after its worker was killed");
    assert!(result.is_err(), "killed worker unexpectedly succeeded");
    assert!(
        wait_for(Duration::from_secs(1), || {
            nested_pids.iter().all(|pid| !running(*pid))
        }),
        "nested subprocess survived outer worker cleanup: {nested_pids:?}"
    );
}

#[test]
fn cancelled_date_preflight_cleans_its_actual_subprocess_and_descendant() {
    cancellation_during_nested_call(false);
}

#[test]
fn cancelled_lifecycle_cli_cleans_its_actual_subprocess_and_descendant() {
    cancellation_during_nested_call(true);
}
