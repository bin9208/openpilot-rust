use openpilot_process_supervision::CapturedCommand;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::fd::AsFd,
    path::PathBuf,
    process::Command,
};

#[test]
fn inherited_child_keeps_terminal_session_and_lock_when_parent_closes() {
    // Given: one real transaction lock and FIFO gates for an owned shell.
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    assert!(Command::new("mkfifo")
        .args([root.join("ready"), root.join("gate")])
        .status()
        .unwrap()
        .success());
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("lock"))
        .unwrap();
    lock.lock().unwrap();
    let flags = rustix::io::fcntl_getfd(&lock).unwrap();
    let request = CapturedCommand {
        launcher: env!("CARGO_BIN_EXE_openpilot-process-child").into(),
        cwd: root.into(),
        argv: [
            "/bin/sh",
            "-c",
            "printf R > ready; head -c1 gate >/dev/null; printf inherited > observed",
        ]
        .map(Into::into)
        .into(),
    };
    // When: inherited terminal execution transfers the lock's open description.
    let mut child = request.spawn_inherited_with_lock(lock.as_fd()).unwrap();
    let pid = rustix::process::Pid::from_raw(i32::try_from(child.process.id()).unwrap()).unwrap();
    let pgid = rustix::process::getpgid(Some(pid)).unwrap();
    assert_eq!(pgid, rustix::process::getpgrp());
    assert_eq!(rustix::io::fcntl_getfd(&lock).unwrap(), flags);
    let mut ready = [0];
    OpenOptions::new()
        .read(true)
        .open(root.join("ready"))
        .unwrap()
        .read_exact(&mut ready)
        .unwrap();
    assert_eq!(ready, *b"R");
    drop(lock);
    // Then: the lock remains owned by the shell until it exits.
    let contender = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("lock"))
        .unwrap();
    assert!(matches!(
        contender.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    OpenOptions::new()
        .write(true)
        .open(root.join("gate"))
        .unwrap()
        .write_all(b"G")
        .unwrap();
    assert!(child.process.wait().unwrap().success());
    assert_eq!(fs::read(root.join("observed")).unwrap(), b"inherited");
    contender.try_lock().unwrap();
    let evidence = std::env::var_os("CI_JOB_TEMP")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.to_path_buf())
        .join("test-evidence/carrot-recovery-263/actions-native-lock");
    fs::create_dir_all(&evidence).unwrap();
    fs::write(
        evidence.join("inherited-lock.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "scenario": "inherited child terminal session and transaction lock",
            "pid": child.process.id(), "pgid": pgid.as_raw_nonzero().get(),
            "parent_pgid": rustix::process::getpgrp().as_raw_nonzero().get(),
            "parent_cloexec_preserved": true, "held_after_parent_close": true,
            "exit": 0, "lock_released_after_exit": true, "observed": "inherited"
        }))
        .unwrap(),
    )
    .unwrap();
}
