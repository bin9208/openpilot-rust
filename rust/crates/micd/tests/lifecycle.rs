use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn library(root: &Path) -> PathBuf {
    let path = root.join("libmic-fixture.so");
    let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args([
            "-std=c11", "-shared", "-fPIC", "-Wall", "-Wextra", "-Werror", "-pthread",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/fixtures/micd/portaudio.c"))
        .arg("-o")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    path
}

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn capture_lifecycle(setting: Option<&str>, expected: Option<&str>) {
    let root = tempfile::tempdir().unwrap();
    let shm = tempfile::Builder::new()
        .prefix("msgq_rust-probe-mic-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = shm
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let events = root.path().join("events.jsonl");
    fs::write(root.path().join("go"), "").unwrap();
    let stderr = root.path().join("stderr");
    let mut command = Command::new(env!("CARGO_BIN_EXE_openpilot-micd"));
    if let Some(setting) = setting {
        command.env(setting, "1");
    } else {
        command.args(["--cycles", "12"]);
    }
    let mut child = Owned(
        command
            .args(["--portaudio"])
            .arg(library(root.path()))
            .env_remove("MANAGER_DAEMON")
            .env("OPENPILOT_PREFIX", prefix)
            .env("MIC_FIXTURE_EVENTS", &events)
            .env("MIC_FIXTURE_GO", root.path().join("go"))
            .stdout(Stdio::null())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "micd kept publishing after capture failure: {}",
            fs::read_to_string(&events).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(status.code(), Some(i32::from(expected.is_some())));
    if let Some(expected) = expected {
        assert!(fs::read_to_string(stderr).unwrap().contains(expected));
    }
    let events: Vec<serde_json::Value> = fs::read_to_string(events)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let names: Vec<_> = events
        .iter()
        .map(|row| row["event"].as_str().unwrap())
        .collect();
    assert_eq!(&names[names.len() - 3..], &["stop", "close", "terminate"]);
    assert!(events.iter().any(|row| row["event"] == "poll"));
    assert_eq!(
        events
            .iter()
            .any(|row| row["event"] == "poll" && row["value"].as_i64().unwrap() <= 0),
        expected.is_some()
    );
}

#[test]
fn inactive_capture_exits_for_supervisor_restart() {
    capture_lifecycle(Some("MIC_FIXTURE_INACTIVE"), Some("micd stream inactive"));
}

#[test]
fn aborted_callback_exits_for_supervisor_restart() {
    capture_lifecycle(
        Some("MIC_FIXTURE_ABORT_CALLBACK"),
        Some("micd stream inactive"),
    );
}

#[test]
fn activity_query_error_is_reported_as_portaudio_failure() {
    capture_lifecycle(
        Some("MIC_FIXTURE_FAIL_ACTIVE"),
        Some("PortAudio active: error -9985"),
    );
}

#[test]
fn normal_cycle_completion_remains_successful() {
    capture_lifecycle(None, None);
}
