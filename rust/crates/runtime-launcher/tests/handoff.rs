use std::{fs::File, os::unix::net::UnixStream, process::Command, sync::atomic::AtomicBool};

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
