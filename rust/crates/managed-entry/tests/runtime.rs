use openpilot_managed_entry::{Outcome, StepError};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};

fn owned_child() -> Command {
    let executable = std::env::current_exe().unwrap();
    if let Some(runner) = std::env::var_os("MANAGED_ENTRY_TEST_LAUNCHER") {
        let mut command = Command::new(runner);
        command.arg(executable);
        command
    } else {
        Command::new(executable)
    }
}

#[test]
#[ignore = "executed by the isolated runtime boundary test"]
fn runtime_child() {
    let outcome = openpilot_managed_entry::runtime("openpilot.system.statsd", "statsd", || {
        match std::env::var("OWNED_ENTRY_MODE").unwrap().as_str() {
            "failure" => Err(StepError::raised(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "owned input missing",
            ))),
            "interrupt" => Err(StepError::Interrupted),
            "return" => Ok(()),
            _ => panic!("unknown fixture"),
        }
    });
    match std::env::var("OWNED_ENTRY_MODE").unwrap().as_str() {
        "failure" => assert!(outcome
            .unwrap_err()
            .to_string()
            .contains("owned input missing")),
        "interrupt" => assert_eq!(outcome.unwrap(), Outcome::Interrupted),
        "return" => assert_eq!(outcome.unwrap(), Outcome::Returned),
        _ => unreachable!(),
    }
    std::io::stdin().read_exact(&mut [0]).unwrap();
}

#[test]
fn real_child_records_typed_failure_and_interrupt_with_inherited_context() {
    for mode in ["failure", "interrupt"] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("build.json"), "{}").unwrap();
        let params = root.path().join("params");
        let prefix = format!("-managed-{}-{mode}", std::process::id());
        let ipc = format!("/tmp/logmessage{prefix}");
        let context = zmq::Context::new();
        let pull = context.socket(zmq::PULL).unwrap();
        pull.set_rcvtimeo(5000).unwrap();
        pull.bind(&format!("ipc://{ipc}")).unwrap();
        let mut child = owned_child()
            .args(["--exact", "runtime_child", "--ignored"])
            .env("MANAGER_DAEMON", "statsd")
            .env("OPENPILOT_BASEDIR", root.path())
            .env("PARAMS_ROOT", &params)
            .env("OPENPILOT_PREFIX", &prefix)
            .env(
                "OPENPILOT_LOG_CONTEXT",
                r#"{"dongle_id":"owned-device","version":"owned-version"}"#,
            )
            .env("OWNED_ENTRY_MODE", mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let packet = pull.recv_bytes(0);
        if packet.is_ok() {
            child.stdin.take().unwrap().write_all(b"x").unwrap();
        } else {
            child.kill().unwrap();
        }
        let status = child.wait().unwrap();
        drop(pull);
        let _ = std::fs::remove_file(ipc);
        let packet = packet.unwrap();
        assert!(status.success());
        let record: serde_json::Value = serde_json::from_slice(&packet[1..]).unwrap();
        assert_eq!(record["ctx"]["daemon"], "statsd");
        assert_eq!(record["ctx"]["dongle_id"], "owned-device");
        assert_eq!(record["ctx"]["runtime_language"], "rust");
        if mode == "failure" {
            assert_eq!(record["msg"], "crash");
            assert!(record.to_string().contains("std::io::error::Error"));
            assert!(record.to_string().contains("owned input missing"));
            let exception = params.join(&prefix).join("CarrotException");
            assert_eq!(std::fs::read(exception).unwrap(), b"exception");
        } else {
            assert_eq!(record["msg"], "child openpilot.system.statsd got SIGINT");
            assert!(!params.exists());
        }
    }
}

#[test]
fn standalone_return_does_not_require_runtime_metadata_or_params() {
    let root = tempfile::tempdir().unwrap();
    let mut child = owned_child()
        .args(["--exact", "runtime_child", "--ignored"])
        .env_remove("MANAGER_DAEMON")
        .env("OPENPILOT_BASEDIR", root.path())
        .env("OWNED_ENTRY_MODE", "return")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"x").unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
