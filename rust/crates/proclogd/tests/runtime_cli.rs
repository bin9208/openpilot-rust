use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn bounded_runtime_publishes_without_waiting_for_a_subscriber() {
    let directory = tempfile::Builder::new()
        .prefix("msgq_runtime-cli-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = directory
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let proc_root = tempfile::tempdir().unwrap();
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_openpilot-proclogd-runtime"))
        .args(["--frames", "1", "--proc-root"])
        .arg(proc_root.path())
        .env("OPENPILOT_PREFIX", prefix)
        .env_remove("CEREAL_FAKE")
        .env_remove("ZMQ")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("collection warnings"));
    assert!(
        fs::metadata(directory.path().join("procLog"))
            .unwrap()
            .len()
            >= 10 * 1024 * 1024
    );
}

#[test]
fn unavailable_proc_root_exits_with_error() {
    let directory = tempfile::Builder::new()
        .prefix("msgq_runtime-error-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = directory
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_openpilot-proclogd-runtime"))
        .args(["--frames", "1", "--proc-root"])
        .arg(directory.path().join("absent"))
        .env("OPENPILOT_PREFIX", prefix)
        .env_remove("CEREAL_FAKE")
        .env_remove("ZMQ")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("No such file"));
}
