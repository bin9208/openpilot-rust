//! No remote SDK: hold the reporting boundary while checking original stderr.
use openpilot_crash_reporting::{
    Configuration, Error, NativeException, ParamsSource, Reporter, RuntimeInputs, Sdk,
};
use openpilot_logging::producer::Factory;
use openpilot_logmessaged::JsonValue;
use openpilot_managed_entry::{launch, StepError};
use std::{
    io::{BufRead, Read, Write},
    process::{Command, Stdio},
    time::Duration,
};
struct Blocked;
impl Sdk for Blocked {
    fn init(&mut self, _: Configuration) -> Result<(), Error> {
        Ok(())
    }
    fn set_user(&mut self, _: Option<String>) -> Result<(), Error> {
        Ok(())
    }
    fn set_tag(&mut self, _: &str, _: &JsonValue) -> Result<(), Error> {
        Ok(())
    }
    fn set_extra(&mut self, _: &str, _: &JsonValue) -> Result<(), Error> {
        Ok(())
    }
    fn capture_message(&mut self, _: &str) -> Result<(), Error> {
        Ok(())
    }
    fn capture_exception(&mut self, _: &NativeException) -> Result<(), Error> {
        println!("SDK BLOCKED");
        std::io::stdout().flush()?;
        std::io::stdin().read_exact(&mut [0])?;
        Ok(())
    }
    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
#[derive(Debug, thiserror::Error)]
#[error("owned managed failure")]
struct Outer {
    #[source]
    source: std::io::Error,
}
#[test]
#[ignore = "native child blocks only inside the inert SDK"]
fn early_child() {
    let root = std::path::PathBuf::from(std::env::var_os("MANAGED_ERROR_FIXTURE_ROOT").unwrap());
    let factory = Factory::new("inproc://early-error-owned".into()).unwrap();
    let mut inputs = RuntimeInputs::new(root.clone());
    inputs.params = ParamsSource::Isolated {
        root: root.join("params"),
        prefix: "owned".into(),
    };
    let mut reporter = Reporter {
        sdk: Blocked,
        inputs,
        logger: factory.logger(),
    };
    let result = launch(
        &mut reporter,
        "owned",
        "owned",
        || {
            Err::<(), _>(StepError::raised(Outer {
                source: std::io::Error::other("owned cause"),
            }))
        },
        |()| Ok(()),
        |(), _| Ok(()),
    );
    assert!(result.is_err());
}
#[test]
fn native_error_is_printed_with_trace_before_blocked_reporting() {
    let root = tempfile::tempdir().unwrap();
    let stderr = root.path().join("stderr");
    let exe = std::env::current_exe().unwrap();
    let mut cmd = if let Some(runner) = std::env::var_os("MANAGED_ENTRY_TEST_LAUNCHER") {
        let mut c = Command::new(runner);
        c.arg(exe);
        c
    } else {
        Command::new(exe)
    };
    let mut child = cmd
        .args(["--exact", "early_child", "--ignored", "--nocapture"])
        .env("MANAGED_ERROR_FIXTURE_ROOT", root.path())
        .env("RUST_BACKTRACE", "0")
        .env("RUST_LIB_BACKTRACE", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            if line.unwrap() == "SDK BLOCKED" {
                let _ = tx.send(());
            }
        }
    });
    let ready = rx.recv_timeout(Duration::from_secs(10));
    let captured = std::fs::read_to_string(&stderr).unwrap();
    if ready.is_ok() {
        child.stdin.take().unwrap().write_all(b"x").unwrap();
    } else {
        let _ = child.kill();
    }
    let status = child.wait().unwrap();
    reader.join().unwrap();
    ready.unwrap();
    assert!(status.success());
    assert!(captured.contains("Managed native failure"), "{captured}");
    assert!(
        captured.contains("owned managed failure") && captured.contains("caused by: owned cause"),
        "{captured}"
    );
    assert!(!captured.contains("disabled backtrace"), "{captured}");
    assert!(captured.contains("0:"), "native frames absent: {captured}");
}
