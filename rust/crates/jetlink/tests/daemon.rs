use openpilot_params::Params;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn command(root: &Path) -> Command {
    // Cargo's runner applies to the test process, not children it launches.
    // Keep the real daemon process tests executable under ARM user emulation.
    let runner = if cfg!(target_arch = "aarch64") {
        std::env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER").ok()
    } else {
        None
    };
    let mut command = if let Some(runner) = runner {
        let mut parts = runner.split_whitespace();
        let mut command = Command::new(parts.next().expect("nonempty Cargo runner"));
        command.args(parts).arg(env!("CARGO_BIN_EXE_jetlinkd-rs"));
        command
    } else {
        Command::new(env!("CARGO_BIN_EXE_jetlinkd-rs"))
    };
    command
        .args(["--run", "--socket"])
        .arg(root.join("owner.sock"))
        .env("PARAMS_ROOT", root.join("params"))
        .env("OPENPILOT_PREFIX", "fixture");
    command
}
#[test]
fn unattached_or_disabled_requests_publish_status_without_usb_setup() {
    for (offroad, mode) in [(true, 0), (false, 2)] {
        // Without a physical cable (or with mode Off), ignition alone does not start setup.
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
            "status={} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let status: serde_json::Value =
            serde_json::from_slice(&params.get("JetlinkStatus").unwrap().unwrap()).unwrap();
        assert_eq!(status["phase"], "OFF");
        let source = openpilot_jetlink::status::read_json(&dir.path().join("carrot-jetlink.json"));
        assert_eq!(source["state"], "waiting");
        assert!(source["updated"].as_f64().unwrap() > 0.);
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
