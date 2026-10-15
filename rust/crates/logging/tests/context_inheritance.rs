use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};

// A QEMU runner is a validation-only boundary; every child still executes the
// freshly linked ARM harness. Production logging and process spawning are unchanged.
fn owned_child() -> Command {
    let executable = std::env::current_exe().unwrap();
    if let Some(runner) = std::env::var_os("LOGGING_TEST_LAUNCHER") {
        let mut command = Command::new(runner);
        command.arg(executable);
        command
    } else {
        Command::new(executable)
    }
}

#[test]
#[ignore = "owned child validates a fresh process logging context"]
fn context_child() {
    if std::env::var("OWNED_CONTEXT_INVALID").as_deref() == Ok("1") {
        assert!(Factory::for_runtime().is_err());
        return;
    }
    let factory = Factory::for_runtime().unwrap();
    let mut logger = factory.logger();
    logger
        .emit(
            log_site!(),
            Record::text(Level::Info, "installed child".into()),
        )
        .unwrap();
    std::io::stdin().read_exact(&mut [0_u8]).unwrap();
}

#[test]
fn exec_child_inherits_original_context_and_names_its_actual_implementation() {
    let prefix = format!(
        "-owned-context-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = format!("/tmp/logmessage{prefix}");
    let context = zmq::Context::new();
    let pull = context.socket(zmq::PULL).unwrap();
    pull.set_rcvtimeo(2000).unwrap();
    pull.bind(&format!("ipc://{path}")).unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut child = owned_child().args(["--exact", "context_child", "--ignored"])
        .env("OPENPILOT_PREFIX", &prefix)
        .env("MANAGER_DAEMON", "calibrationd")
        .env("OPENPILOT_LOG_CONTEXT", r#"{"version":"owned-version","dongle_id":"owned-device","dirty":true,"runtime_implementation":"openpilot-manager","runtime_language":"python","source_commit":"parent-commit","source_tree":"parent-tree"}"#)
        .stdin(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
    let packet = pull.recv_bytes(0);
    if packet.is_err() {
        child.kill().unwrap();
    } else {
        child.stdin.take().unwrap().write_all(b"x").unwrap();
    }
    let status = child.wait().unwrap();
    drop(pull);
    let _ = std::fs::remove_file(path);
    let packet = packet.unwrap();
    assert!(status.success());
    let record: serde_json::Value = serde_json::from_slice(&packet[1..]).unwrap();
    assert_eq!(record["ctx"]["version"], "owned-version");
    assert_eq!(record["ctx"]["dongle_id"], "owned-device");
    assert_eq!(record["ctx"]["dirty"], true);
    assert_eq!(record["ctx"]["daemon"], "calibrationd");
    assert_eq!(record["ctx"]["runtime_language"], "rust");
    assert_ne!(record["ctx"]["source_commit"], "parent-commit");
    assert_ne!(record["ctx"]["source_tree"], "parent-tree");
    assert_eq!(
        record["ctx"]["runtime_implementation"],
        exe.file_name().unwrap().to_str().unwrap()
    );
}

#[test]
fn invalid_inherited_context_does_not_silently_lose_diagnostics() {
    for value in ["broken", "[]", "null"] {
        assert!(owned_child()
            .args(["--exact", "context_child", "--ignored"])
            .env("OPENPILOT_LOG_CONTEXT", value)
            .env("OWNED_CONTEXT_INVALID", "1")
            .stdout(Stdio::null())
            .status()
            .unwrap()
            .success());
    }
}
