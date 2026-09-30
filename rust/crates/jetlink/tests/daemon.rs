use openpilot_params::Params;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jetlinkd-rs"));
    command
        .args(["--run", "--socket"])
        .arg(root.join("owner.sock"))
        .env("PARAMS_ROOT", root.join("params"))
        .env("OPENPILOT_PREFIX", "fixture");
    command
}
#[test]
fn off_and_onroad_requests_publish_status_without_usb_setup() {
    for (offroad, mode) in [(true, 0), (false, 2)] {
        // Given isolated parameters which never authorize gadget setup.
        let dir = tempfile::tempdir().unwrap();
        let params = Params::open(&dir.path().join("params"), "fixture").unwrap();
        params.put_bool("IsOffroad", offroad).unwrap();
        params
            .put("JetlinkMode", mode.to_string().as_bytes())
            .unwrap();
        // When the real daemon performs its normal status iteration.
        let output = command(dir.path())
            .args(["--iterations", "1"])
            .output()
            .unwrap();
        // Then it exits cleanly, reports OFF, and never creates the inference listener.
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let status: serde_json::Value =
            serde_json::from_slice(&params.get("JetlinkStatus").unwrap().unwrap()).unwrap();
        assert_eq!(status["phase"], "OFF");
        assert!(!dir.path().join("owner.sock").exists());
    }
}
#[test]
fn duplicate_owner_is_rejected_and_signal_shutdown_is_bounded() {
    // Given a running real daemon in the off state.
    let dir = tempfile::tempdir().unwrap();
    let params = Params::open(&dir.path().join("params"), "fixture").unwrap();
    params.put_bool("IsOffroad", true).unwrap();
    params.put("JetlinkMode", b"0").unwrap();
    let mut daemon = command(dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while params.get("JetlinkStatus").unwrap().is_none() {
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    // When another owner starts and the original receives SIGTERM.
    let duplicate = command(dir.path())
        .args(["--iterations", "1"])
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert!(Command::new("kill")
        .args(["-TERM", &daemon.id().to_string()])
        .status()
        .unwrap()
        .success());
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = daemon.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    // Then the process releases its exclusive lock for the next normal startup.
    assert!(command(dir.path())
        .args(["--iterations", "1"])
        .status()
        .unwrap()
        .success());
    assert!(fs::metadata(dir.path().join("owner.owner.lock")).is_ok());
}
