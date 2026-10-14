use openpilot_process_supervision::CapturedCommand;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsFd,
        unix::{ffi::OsStringExt, net::UnixStream},
    },
    path::Path,
    process::{Command, Stdio},
};

fn main() {
    if let Some(descriptor) = std::env::args_os()
        .nth(1)
        .filter(|path| std::path::Path::new(path).is_file())
    {
        let request: serde_json::Value =
            serde_json::from_slice(&fs::read(descriptor).unwrap()).unwrap();
        let path = |key: &str| {
            let bytes: Vec<u8> = serde_json::from_value(request[key].clone()).unwrap();
            std::path::PathBuf::from(std::ffi::OsString::from_vec(bytes))
        };
        std::env::set_current_dir(path("cwd")).unwrap();
        let stream = UnixStream::connect(path("handshake")).unwrap();
        rustix::process::setsid().unwrap();
        let mut child = Command::new("/bin/sh")
            .args(["-c", "printf R; read value < gate"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = [0];
        child.stdout.take().unwrap().read_exact(&mut ready).unwrap();
        assert_eq!(ready, *b"R");
        fs::write("descendant-pid", child.id().to_string()).unwrap();
        drop(stream);
        child.wait().unwrap();
        return;
    }
    child_keeps_transaction_after_parent_close_until_exit();
    exec_failure_closes_received_lock();
    old_protocol_helper_fails_closed_and_cleans_its_session();
}

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

fn old_protocol_helper_fails_closed_and_cleans_its_session() {
    let directory = tempfile::tempdir().unwrap();
    let helper = std::env::current_exe().unwrap();
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
    let pid = rustix::process::Pid::from_raw(pid.parse().unwrap()).unwrap();
    match rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()) {
        Ok(fd) => {
            let mut poll = [rustix::event::PollFd::new(
                &fd,
                rustix::event::PollFlags::IN,
            )];
            let timeout = rustix::event::Timespec {
                tv_sec: 1,
                tv_nsec: 0,
            };
            assert_eq!(rustix::event::poll(&mut poll, Some(&timeout)).unwrap(), 1);
            assert!(poll[0].revents().contains(rustix::event::PollFlags::IN));
        }
        Err(rustix::io::Errno::SRCH) => (),
        Err(error) => panic!("descendant pidfd: {error}"),
    }
    drop(file);
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.path().join("repo-lock"))
        .unwrap()
        .try_lock()
        .unwrap();
}
