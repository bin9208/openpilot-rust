use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn native_daemon_publishes_five_camera_absent_ticks_and_exits() {
    let directory = tempfile::tempdir().unwrap();
    let prefix = format!("rust-probe-dm2-process-{}", std::process::id());
    let shm = std::path::PathBuf::from("/dev/shm").join(format!("msgq_{prefix}"));
    std::fs::create_dir(&shm).unwrap();
    let params = openpilot_params::Params::open(directory.path(), &prefix).unwrap();
    params.put_bool("DriverMonitoringEnabled", true).unwrap();
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_openpilot-dmonitoringd"))
        .args(["--frames", "5"])
        .env("PARAMS_ROOT", directory.path())
        .env("OPENPILOT_PREFIX", &prefix)
        .env_remove("MANAGER_DAEMON")
        .env_remove("CEREAL_FAKE")
        .env_remove("REPLAY")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    while child.try_wait().unwrap().is_none() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    if child.try_wait().unwrap().is_none() {
        child.kill().unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let elapsed = started.elapsed();
    std::fs::remove_dir_all(&shm).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("dmonitoringd: ready"));
    assert!(
        elapsed >= Duration::from_millis(200),
        "camera loss must not create catch-up publication bursts: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "camera loss must not hang the daemon: {elapsed:?}"
    );
}
