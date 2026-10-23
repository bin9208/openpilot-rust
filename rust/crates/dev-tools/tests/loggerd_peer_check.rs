//! New hosted controls for the remaining Python idle-observer consumer.
#![cfg(target_os = "linux")]
use openpilot_dev_tools::loggerd_check::observe_logger_idle;
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const CHECKER: &str = env!("CARGO_BIN_EXE_check-loggerd-peer");
const FIXTURE: &str = env!("CARGO_BIN_EXE_loggerd-lifecycle-fixture");
fn evidence() -> PathBuf {
    let parent = std::env::var_os("LOGGERD_PEER_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    fs::create_dir_all(&parent).unwrap();
    tempfile::Builder::new()
        .prefix("loggerd-peer-")
        .tempdir_in(parent)
        .unwrap()
        .keep()
}
struct OwnedChild {
    child: Child,
    reaped: bool,
}
impl OwnedChild {
    fn finish(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            self.child.wait().unwrap();
            self.reaped = true;
        }
    }
    fn running(&mut self) -> bool {
        if self.child.try_wait().unwrap().is_some() {
            self.reaped = true;
            false
        } else {
            true
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
#[test]
fn actual_kernel_wait_states_keep_the_original_idle_admission() {
    for mode in [
        "sleep",
        "zero-ppoll",
        "fd-ppoll",
        "pipe-read",
        "futex",
        "exited",
    ] {
        let root = evidence();
        let ready = root.join("ready");
        let child = Command::new(FIXTURE)
            .args(["--idle-observer", mode])
            .arg(&ready)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(root.join("stderr.log")).unwrap())
            .spawn()
            .unwrap();
        let mut owned = OwnedChild {
            child,
            reaped: false,
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while !ready.exists() {
            assert!(
                owned.running() && Instant::now() < deadline,
                "fixture readiness: {mode}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut row = json!({"mode":mode,"pid":owned.child.id()});
        if mode == "exited" {
            while owned.running() {
                assert!(Instant::now() < deadline, "exited fixture remained live");
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(observe_logger_idle(&mut owned.child, Duration::from_millis(40)).is_err());
            row["observer"] = json!("process exit rejected");
        } else {
            let proc = PathBuf::from(format!("/proc/{}", owned.child.id()));
            loop {
                assert!(
                    owned.running() && Instant::now() < deadline,
                    "kernel wait readiness: {mode}"
                );
                let channel = fs::read_to_string(proc.join("wchan")).unwrap();
                let syscall = fs::read_to_string(proc.join("syscall")).unwrap();
                if !["", "0"].contains(&channel.trim())
                    && !["", "running"].contains(&syscall.trim())
                {
                    row["wchan"] = json!(channel.trim());
                    row["syscall"] = json!(syscall.trim());
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            let result = observe_logger_idle(&mut owned.child, Duration::from_millis(40));
            if ["sleep", "zero-ppoll"].contains(&mode) {
                result.unwrap();
                row["observer"] = json!("idle accepted");
            } else {
                let error = result.unwrap_err().to_string();
                assert!(
                    error.contains("did not complete processing"),
                    "{mode}: {error}"
                );
                assert!(
                    owned.running(),
                    "rejected an exited child instead of a live non-idle wait"
                );
                row["observer"] = json!("non-idle timeout preserved");
                row["error"] = json!(error);
            }
        }
        owned.finish();
        row["fixture_reaped"] = json!(owned.reaped);
        fs::write(
            root.join("observation.json"),
            serde_json::to_vec_pretty(&row).unwrap(),
        )
        .unwrap();
    }
}
#[test]
fn standalone_peer_checks_real_processing_and_owned_namespace_cleanup() {
    let root = evidence();
    let output = root.join("comparison");
    let status = Command::new(CHECKER)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let result: Value =
        serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&status.stdout).unwrap(),
        result
    );
    assert_eq!(result["packet_exact"], true);
    assert_eq!(result["processing_finished_before_send_return"], true);
    let time = |name: &str| result[name].as_u64().unwrap();
    assert!(time("started") <= time("received") && time("received") <= time("completed"));
    assert!(time("completed") <= time("send_returned"));
    assert!(time("completed") - time("received") >= 100_000_000);
    assert_eq!(result["argv"][1], "--internal-loggerd-idle-fixture");
    let cleanup: Value =
        serde_json::from_slice(&fs::read(output.join("control/cleanup.json")).unwrap()).unwrap();
    assert_eq!(cleanup["processes_confirmed"], true);
    assert_eq!(cleanup["error"], Value::Null);
    assert_eq!(cleanup["pending_namespaces"], 0);
    assert_eq!(cleanup["pending_receivers"], 0);
    assert_eq!(cleanup["pending_endpoints"], 0);
    for event in cleanup["events"].as_array().unwrap() {
        if event["phase"] == "ipc-cleanup" {
            assert_eq!(event["resource"]["status"], "removed-owned");
            assert!(!PathBuf::from(event["resource"]["path"].as_str().unwrap()).exists());
        }
    }
    assert_eq!(
        fs::read(output.join("providers.before.json")).unwrap(),
        fs::read(output.join("providers.after.json")).unwrap()
    );
}
#[test]
fn standalone_peer_rejects_arguments_and_foreign_output_before_work() {
    for args in [vec!["--help"], vec![], vec!["--unknown"]] {
        let output = Command::new(CHECKER).args(&args).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(if args == ["--help"] { 0 } else { 2 })
        );
    }
    let root = evidence();
    fs::write(root.join("foreign"), b"unchanged").unwrap();
    let result = Command::new(CHECKER)
        .arg("--output")
        .arg(&root)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(root.join("foreign")).unwrap(), b"unchanged");
    assert!(!root.join("control").exists());
}
