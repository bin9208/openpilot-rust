use openpilot_process_supervision::CapturedCommand;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{fd::AsFd, unix::fs::PermissionsExt},
    path::Path,
    process::Command,
};

fn request(root: &Path, argv: &[&str]) -> CapturedCommand {
    CapturedCommand {
        launcher: env!("CARGO_BIN_EXE_openpilot-process-child").into(),
        cwd: root.into(),
        argv: argv.iter().map(Into::into).collect(),
    }
}

fn locked(root: &Path) -> File {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("repo-lock"))
        .unwrap();
    file.lock().unwrap();
    file
}

#[test]
fn child_keeps_transaction_after_parent_close_until_exit() {
    let directory = tempfile::tempdir().unwrap();
    assert!(Command::new("mkfifo")
        .arg(directory.path().join("gate"))
        .status()
        .unwrap()
        .success());
    let file = locked(directory.path());
    let (mut child, mut pipe) = request(
        directory.path(),
        &["/bin/sh", "-c", "printf R; head -c1 gate >/dev/null"],
    )
    .spawn_session_merged_with_lock(&[], file.as_fd())
    .unwrap();
    let mut ready = [0];
    pipe.read_exact(&mut ready).unwrap();
    assert_eq!(ready, *b"R");
    drop(file);
    let contender = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.path().join("repo-lock"))
        .unwrap();
    assert!(matches!(
        contender.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    OpenOptions::new()
        .write(true)
        .open(directory.path().join("gate"))
        .unwrap()
        .write_all(b"G")
        .unwrap();
    assert!(child.process.wait().unwrap().success());
    contender.try_lock().unwrap();
}

#[test]
fn exec_failure_closes_received_lock() {
    let directory = tempfile::tempdir().unwrap();
    let file = locked(directory.path());
    assert!(
        request(directory.path(), &["/missing-owned-git-status-command"])
            .spawn_session_merged_with_lock(&[], file.as_fd())
            .is_err()
    );
    drop(file);
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.path().join("repo-lock"))
        .unwrap()
        .try_lock()
        .unwrap();
}

#[test]
fn old_protocol_helper_fails_closed_and_cleans_its_session() {
    let directory = tempfile::tempdir().unwrap();
    let helper = directory.path().join("old-helper");
    fs::write(
        &helper,
        br##"#!/usr/bin/python3
import json, os, socket, subprocess, sys
request = json.load(open(sys.argv[1]))
os.chdir(bytes(request['cwd']))
stream = socket.socket(socket.AF_UNIX)
stream.connect(bytes(request['handshake']))
os.setsid()
child = subprocess.Popen(['/bin/sh', '-c', 'printf R; read value < gate'], stdout=subprocess.PIPE)
assert child.stdout.read(1) == b'R'
open('descendant-pid', 'w').write(str(child.pid))
stream.close()
child.wait()
"##,
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(Command::new("mkfifo")
        .arg(directory.path().join("gate"))
        .status()
        .unwrap()
        .success());
    let file = locked(directory.path());
    let mut command = request(directory.path(), &["/bin/true"]);
    command.launcher = helper;
    assert!(command
        .spawn_session_merged_with_lock(&[], file.as_fd())
        .is_err());
    let pid = fs::read_to_string(directory.path().join("descendant-pid")).unwrap();
    let output = Command::new("python3").args(["-c", "import os,select,sys\ntry: fd=os.pidfd_open(int(sys.argv[1]))\nexcept ProcessLookupError: print('exited')\nelse:\n p=select.poll();p.register(fd,select.POLLIN);assert p.poll(1000);print('exited')", &pid]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"exited\n");
    drop(file);
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.path().join("repo-lock"))
        .unwrap()
        .try_lock()
        .unwrap();
}
