//! CLI/process boundaries run in fresh processes to isolate global IPC prefixes.
#![cfg(feature = "encoder-runtime-native")]
use std::{fs, process::Command};
#[test]
fn actual_cli_help_and_failed_child_leave_no_owned_namespace() {
    let executable = env!("CARGO_BIN_EXE_check-encoder-runtime");
    assert!(Command::new(executable)
        .arg("--help")
        .status()
        .unwrap()
        .success());
    let tmp = tempfile::tempdir().unwrap();
    let output = tmp.path().join("run");
    let result = Command::new(executable)
        .args([
            "--source",
            "/nonexistent/owned-encoder",
            "--rust",
            "/nonexistent/owned-rust",
            "--native",
            "/nonexistent/native",
            "--case=main",
        ])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.join("receipt.json").exists());
    let invocation = output.join("case-0-source/invocation.json");
    // Hashing a missing encoder may fail before the invocation is complete. Params
    // identifies the owned prefix even on that failure, and cleanup must remove IPC.
    let params = output.join("case-0-source/params");
    for entry in fs::read_dir(params).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        assert!(!std::path::Path::new("/dev/shm")
            .join(format!("msgq_{name}"))
            .exists());
        assert!(!std::path::Path::new("/tmp")
            .join(format!("{name}_visionipc_camerad"))
            .exists());
    }
    assert!(!invocation.exists());
    assert_eq!(
        fs::read_dir(output.join("case-0-source/params"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn spawned_failed_encoder_is_reaped_and_owned_ipc_released() {
    let tmp = tempfile::tempdir().unwrap();
    let output = tmp.path().join("failed");
    let result = Command::new(env!("CARGO_BIN_EXE_check-encoder-runtime"))
        .args([
            "--source",
            "/bin/false",
            "--rust",
            "/bin/false",
            "--native",
            "/",
            "--case=main",
        ])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    let directory = output.join("case-0-source");
    let shutdown: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("shutdown.json")).unwrap()).unwrap();
    assert_eq!(shutdown["returncode"], 1);
    assert_eq!(shutdown["timed_out"], false);
    assert!(!std::path::Path::new(&format!("/proc/{}", shutdown["pid"])).exists());
    let invocation: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("invocation.json")).unwrap()).unwrap();
    let prefix = invocation["prefix"].as_str().unwrap();
    assert!(!std::path::Path::new(&format!("/dev/shm/msgq_{prefix}")).exists());
    assert!(!std::path::Path::new(&format!("/tmp/{prefix}_visionipc_camerad")).exists());
    assert!(!output.join("receipt.json").exists());
}
