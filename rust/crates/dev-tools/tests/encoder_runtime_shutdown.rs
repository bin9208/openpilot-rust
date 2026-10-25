//! Original cleanup acceptance applied to real owned native children.
#![cfg(feature = "encoder-runtime-native")]
use openpilot_dev_tools::encoder_runtime_process::stop_cleanly;
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    time::Duration,
};
#[test]
fn real_children_preserve_clean_exit_failure_and_timeout_receipts() {
    for (name, script, code, timed_out, signaled) in [
        (
            "clean",
            "trap 'exit 0' TERM; echo ready; while :; do sleep 0.05; done",
            0,
            false,
            true,
        ),
        ("failed", "exit 23", 23, false, false),
        (
            "ignore",
            "trap '' TERM; echo ready; exec sleep 10",
            -9,
            true,
            true,
        ),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let receipt = tmp.path().join("shutdown.json");
        let mut child = Command::new("/bin/sh")
            .args(["-c", script])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let id = child.id();
        if name == "failed" {
            child.wait().unwrap();
        } else {
            let mut line = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut line)
                .unwrap();
            assert_eq!(line, "ready\n");
        }
        let result = stop_cleanly(&mut child, &receipt, Duration::from_millis(200));
        // Test cleanup also runs on a failed implementation contract.
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(result.is_ok(), name == "clean", "{name}: {result:?}");
        let row: Value = serde_json::from_slice(&fs::read(receipt).unwrap()).unwrap();
        assert_eq!(
            row,
            serde_json::json!({"pid":id,"signaled":signaled,"timed_out":timed_out,"returncode":code})
        );
        assert!(!std::path::Path::new(&format!("/proc/{id}")).exists());
    }
}
