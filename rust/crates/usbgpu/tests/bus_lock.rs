use openpilot_usbgpu::bus_lock::BusLock;
use std::{fs::OpenOptions, path::PathBuf, process::Command, sync::mpsc, time::Duration};
#[test]
fn child_lock_process() {
    let Some(path) = std::env::var_os("USBGPU_TEST_LOCK") else {
        return;
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let result = rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive);
    if std::env::var_os("USBGPU_TEST_EXPECT_BLOCKED").is_some() {
        assert_eq!(result.unwrap_err(), rustix::io::Errno::WOULDBLOCK);
    } else {
        result.unwrap();
    }
}
fn child(path: PathBuf, blocked: bool) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "child_lock_process"])
        .env("USBGPU_TEST_LOCK", path);
    if blocked {
        command.env("USBGPU_TEST_EXPECT_BLOCKED", "1");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
#[test]
fn nested_locks_exclude_threads_and_processes_until_outer_drop() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lock");
    let lock = BusLock::open(&path).unwrap();
    let same = BusLock::open(&path).unwrap();
    let outer = lock.enter().unwrap();
    let inner = same.enter().unwrap();
    child(path.clone(), true);
    drop(inner);
    child(path.clone(), true);
    let (ready_tx, ready_rx) = mpsc::channel();
    let (held_tx, held_rx) = mpsc::channel();
    let cloned = lock.clone();
    let thread = std::thread::spawn(move || {
        ready_tx.send(()).unwrap();
        let _guard = cloned.enter().unwrap();
        held_tx.send(()).unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(held_rx.recv_timeout(Duration::from_millis(30)).is_err());
    drop(outer);
    held_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    thread.join().unwrap();
    child(path, false);
}
