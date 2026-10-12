use openpilot_process_supervision::CapturedCommand;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::fd::AsFd,
    path::Path,
    process::Command,
};

#[test]
fn separate_pipes_preserve_lock_and_parent_session() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    assert!(Command::new("mkfifo")
        .arg(root.join("gate"))
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
            "printf R; head -c1 gate >/dev/null; printf out; printf err >&2",
        ]
        .iter()
        .map(Into::into)
        .collect(),
    };
    let mut child = request.spawn_captured_with_lock(&[], lock.as_fd()).unwrap();
    let pid = rustix::process::Pid::from_raw(i32::try_from(child.process.id()).unwrap()).unwrap();
    let group = rustix::process::getpgid(Some(pid)).unwrap();
    assert_eq!(group, rustix::process::getpgrp());
    assert_eq!(rustix::io::fcntl_getfd(&lock).unwrap(), flags);
    let mut stdout = child.process.stdout.take().unwrap();
    let mut ready = [0];
    stdout.read_exact(&mut ready).unwrap();
    assert_eq!(ready, *b"R");
    drop(lock);
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
    let mut out = Vec::new();
    let mut err = Vec::new();
    stdout.read_to_end(&mut out).unwrap();
    child
        .process
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut err)
        .unwrap();
    assert!(child.process.wait().unwrap().success());
    assert_eq!(out, b"out");
    assert_eq!(err, b"err");
    contender.try_lock().unwrap();
    let evidence = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../.omo/evidence/225-git-config/captured-lock");
    fs::create_dir_all(&evidence).unwrap();
    fs::write(
        evidence.join("observations.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "scenario": "separate pipes/non-session lock inheritance",
            "pid": child.process.id(), "pgid": group.as_raw_nonzero().get(),
            "parent_pgid": rustix::process::getpgrp().as_raw_nonzero().get(),
            "parent_cloexec_preserved": true, "held_after_parent_close": true,
            "stdout": out, "stderr": err, "child_exit": 0, "lock_released_after_exit": true
        }))
        .unwrap(),
    )
    .unwrap();
}
