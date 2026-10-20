use std::{
    fs::File,
    os::{fd::AsFd, unix::net::UnixStream},
    process::Command,
    sync::atomic::AtomicBool,
};

#[test]
#[ignore = "owned process receives the actual launcher lock"]
fn manager_child() {
    let mut stream =
        UnixStream::connect(std::env::var_os("CARROT_BOOT_LOCK_SOCKET").unwrap()).unwrap();
    let lock = openpilot_process_supervision::receive_boot_lock(&mut stream).unwrap();
    let contender = File::open(std::env::var_os("OWNED_LOCK_PATH").unwrap()).unwrap();
    assert!(matches!(
        contender.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    lock.unlock().unwrap();
    contender.try_lock().unwrap();
}

#[test]
fn actual_manager_handoff_releases_the_launcher_lock() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("lock");
    let lock = File::create(&path).unwrap();
    lock.lock().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "manager_child", "--ignored", "--nocapture"])
        .env("OWNED_LOCK_PATH", &path);
    assert!(openpilot_runtime_launcher::handoff::run_locked(
        command,
        &lock,
        &AtomicBool::new(false)
    )
    .unwrap()
    .success());
    File::open(path).unwrap().try_lock().unwrap();
}

#[test]
fn early_child_exit_is_reaped_without_waiting_for_a_missing_handshake() {
    let root = tempfile::tempdir().unwrap();
    let lock = File::create(root.path().join("lock")).unwrap();
    lock.lock().unwrap();
    let command = Command::new("/bin/false");
    let started = std::time::Instant::now();
    assert!(openpilot_runtime_launcher::handoff::run_locked(
        command,
        &lock,
        &AtomicBool::new(false)
    )
    .is_err());
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
#[ignore = "owned child refuses the lock acknowledgement and ignores TERM"]
fn resistant_handoff_child() {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        process::{pidfd_open, Pid, PidfdFlags},
    };
    let root = std::path::PathBuf::from(std::env::var_os("OWNED_ERROR_ROOT").unwrap());
    let token = std::env::var("OWNED_ERROR_TOKEN").unwrap();
    let parent: i32 = std::env::var("OWNED_ERROR_PARENT")
        .unwrap()
        .parse()
        .unwrap();
    let parent = pidfd_open(Pid::from_raw(parent).unwrap(), PidfdFlags::empty()).unwrap();
    let _signals = signal_hook::iterator::Signals::new([signal_hook::consts::SIGTERM]).unwrap();
    // Keep the socket open without reading the sent descriptor or acknowledging
    // it. The launcher reaches its real two-second send_boot_lock error path.
    let _stream =
        UnixStream::connect(std::env::var_os("CARROT_BOOT_LOCK_SOCKET").unwrap()).unwrap();
    let marker = root.join("ready.pending");
    std::fs::write(&marker, format!("{token} {}\n", std::process::id())).unwrap();
    std::fs::rename(marker, root.join("ready")).unwrap();
    while poll(
        &mut [PollFd::new(&parent, PollFlags::IN)],
        Some(&Timespec::default()),
    )
    .unwrap()
        == 0
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn failed_lock_acknowledgement_kills_and_reaps_term_resistant_owned_child() {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        process::{
            pidfd_open, pidfd_send_signal, waitid, Pid, PidfdFlags, Signal, WaitId, WaitIdOptions,
        },
    };
    let root = tempfile::tempdir().unwrap();
    let lock_path = root.path().join("lock");
    let lock = File::create(&lock_path).unwrap();
    lock.lock().unwrap();
    let token = root
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let marker = root.path().join("ready");
    let expected = token.clone();
    let watcher = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Ok(text) = std::fs::read_to_string(&marker) {
                let fields = text.split_whitespace().collect::<Vec<_>>();
                if fields.len() == 2 && fields[0] == expected {
                    let raw: i32 = fields[1].parse().unwrap();
                    let pid = Pid::from_raw(raw).unwrap();
                    let id = pidfd_open(pid, PidfdFlags::empty()).unwrap();
                    let environment = std::fs::read(format!("/proc/{raw}/environ")).unwrap();
                    assert!(environment
                        .split(|v| *v == 0)
                        .any(|v| v == format!("OWNED_ERROR_TOKEN={expected}").as_bytes()));
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
                    loop {
                        if poll(
                            &mut [PollFd::new(&id, PollFlags::IN)],
                            Some(&Timespec::default()),
                        )
                        .unwrap()
                            != 0
                        {
                            return (id, false);
                        }
                        if std::time::Instant::now() >= deadline {
                            // A baseline launcher may wait forever after TERM.
                            // Kill only the token-verified child held by this pidfd.
                            match pidfd_send_signal(&id, Signal::KILL) {
                                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                                Err(error) => panic!("owned child cleanup failed: {error}"),
                            }
                            return (id, true);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "owned child marker absent"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "resistant_handoff_child",
            "--ignored",
            "--nocapture",
        ])
        .env("OWNED_ERROR_ROOT", root.path())
        .env("OWNED_ERROR_TOKEN", &token)
        .env("OWNED_ERROR_PARENT", std::process::id().to_string());
    let started = std::time::Instant::now();
    let result =
        openpilot_runtime_launcher::handoff::run_locked(command, &lock, &AtomicBool::new(false));
    let elapsed = started.elapsed();
    let (id, forced_cleanup) = watcher.join().unwrap();
    assert!(result.is_err(), "missing lock acknowledgement must fail");
    assert!(
        elapsed >= std::time::Duration::from_millis(1800),
        "handoff failed before its two-second acknowledgement deadline: {elapsed:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "error cleanup exceeded bound: {elapsed:?}"
    );
    assert!(!forced_cleanup, "watcher had to kill the owned child");
    assert_eq!(
        poll(
            &mut [PollFd::new(&id, PollFlags::IN)],
            Some(&Timespec::default())
        )
        .unwrap(),
        1
    );
    assert!(
        matches!(
            waitid(
                WaitId::PidFd(id.as_fd()),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT
            ),
            Err(rustix::io::Errno::CHILD)
        ),
        "owned child was not reaped"
    );
    assert!(matches!(
        File::open(&lock_path).unwrap().try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
}

#[test]
#[ignore = "owned resistant startup child"]
fn slow_manager_child() {
    let root = std::path::PathBuf::from(std::env::var_os("OWNED_SLOW_ROOT").unwrap());
    let mut signals = signal_hook::iterator::Signals::new([signal_hook::consts::SIGTERM]).unwrap();
    let mut stream =
        UnixStream::connect(std::env::var_os("CARROT_BOOT_LOCK_SOCKET").unwrap()).unwrap();
    let lock = openpilot_process_supervision::receive_boot_lock(&mut stream).unwrap();
    std::fs::write(root.join("ready"), std::process::id().to_string()).unwrap();
    signals.forever().next().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(3));
    lock.unlock().unwrap();
    std::fs::write(root.join("cleanup"), "finished after original signal\n").unwrap();
}

#[test]
fn manager_handoff_waits_for_cleanup_longer_than_server_deadline() {
    let root = tempfile::tempdir().unwrap();
    let lock = File::create(root.path().join("lock")).unwrap();
    lock.lock().unwrap();
    let stopped = std::sync::Arc::new(AtomicBool::new(false));
    let trigger = stopped.clone();
    let ready = root.path().join("ready");
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !ready.exists() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        trigger.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "slow_manager_child", "--ignored", "--nocapture"])
        .env("OWNED_SLOW_ROOT", root.path());
    let started = std::time::Instant::now();
    let status = openpilot_runtime_launcher::handoff::run_locked(command, &lock, &stopped).unwrap();
    worker.join().unwrap();
    let elapsed = started.elapsed().as_secs_f64();
    let pid = std::fs::read_to_string(root.path().join("ready")).unwrap();
    let complete = root.path().join("cleanup").exists();
    println!(
        "{}",
        serde_json::json!({"exit":status.to_string(),"cleanup_seconds":elapsed,"cleanup_complete":complete,"child_pid":pid,"child_reaped":!std::path::Path::new(&format!("/proc/{pid}")).exists()})
    );
    assert!(status.success());
    assert!(complete);
    assert!(elapsed >= 3.0);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}
