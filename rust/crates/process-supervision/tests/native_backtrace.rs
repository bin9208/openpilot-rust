//! Actual exec boundary: standard Rust panic diagnostics survive translation.
use std::{path::Path, process::Command};
#[test]
#[ignore = "owned child deliberately panics"]
fn panic_child() {
    eprintln!("daemon={}", std::env::var("MANAGER_DAEMON").unwrap());
    panic!("owned managed panic");
}
fn command(program: &Path) -> (Command, Vec<Vec<u8>>) {
    if let Some(runner) = std::env::var_os("PROCESS_CHILD_TEST_LAUNCHER") {
        let mut command = Command::new(&runner);
        command.arg(program);
        (
            command,
            vec![
                runner.as_encoded_bytes().to_vec(),
                program.as_os_str().as_encoded_bytes().to_vec(),
            ],
        )
    } else {
        (
            Command::new(program),
            vec![program.as_os_str().as_encoded_bytes().to_vec()],
        )
    }
}
#[test]
fn named_child_defaults_to_native_panic_stack_and_preserves_explicit_setting() {
    let root = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    let (_, mut argv) = command(&executable);
    argv.extend(
        ["--exact", "panic_child", "--ignored", "--nocapture"].map(|v| v.as_bytes().to_vec()),
    );
    let descriptor = root.path().join("launch.json");
    std::fs::write(&descriptor, serde_json::to_vec(&serde_json::json!({
        "mode":"native", "name":"crash_fixture", "cwd":root.path().as_os_str().as_encoded_bytes(), "argv":argv,
    })).unwrap()).unwrap();
    for setting in [None, Some("0"), Some("full")] {
        let (mut child, _) = command(Path::new(env!("CARGO_BIN_EXE_openpilot-process-child")));
        child
            .arg(&descriptor)
            .env_remove("RUST_BACKTRACE")
            .env_remove("RUST_LIB_BACKTRACE");
        if let Some(value) = setting {
            child.env("RUST_BACKTRACE", value);
        }
        let output = child.output().unwrap();
        assert_eq!(output.status.code(), Some(101));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("daemon=crash_fixture") && stderr.contains("owned managed panic"),
            "{stderr}"
        );
        assert_eq!(
            stderr.contains("stack backtrace:"),
            setting != Some("0"),
            "setting={setting:?}: {stderr}"
        );
        if setting != Some("0") {
            assert!(stderr.contains("panic_child"), "{stderr}");
        }
    }
}
