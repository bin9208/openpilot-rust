use openpilot_dev_tools::{
    bluetooth_bluez_policy_check::{self as policy, Options},
    qa_process::{CaptureError, Captured, CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::JsonView;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    time::Duration,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn option(output: PathBuf) -> Options {
    Options {
        root: root(),
        binary: root().join("rust/crates/bluetooth/examples/bluez_policy.rs"),
        output,
    }
}
#[test]
fn schedule_is_exhaustive_and_keeps_python_boundary_values() {
    let (values, digits) = policy::values_and_digits().unwrap();
    assert_eq!(values.len(), 909);
    assert_eq!(digits.len(), 808);
    assert_eq!(digits.iter().filter(|(_, d)| *d == Some(0)).count(), 68);
    assert!(matches!(values[8].view(), JsonView::Integer(number) if number.len() == 101));
    assert!(matches!(values[11].view(), JsonView::Float(value) if value.is_infinite()));
    assert!(matches!(values[12].view(), JsonView::Float(value) if value.is_nan()));
    assert!(matches!(values[27].view(), JsonView::Text(points) if points == [0xd800]));
    assert!(matches!(values[29].view(), JsonView::Text(points) if points.len() == 16));
    assert!(matches!(values[30].view(), JsonView::Text(points) if points.len() == 17));
    assert!(matches!(values[31].view(), JsonView::Text(points) if points.len() == 16));
    assert!(matches!(values[32].view(), JsonView::Text(points) if points.len() == 17));
    assert!(matches!(values[33].view(), JsonView::Text(points) if points.len() == 1));
    assert!(matches!(values.last().unwrap().view(), JsonView::Text(points) if points.len() == 6));
}

struct RejectRun;
impl Runner for RejectRun {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        panic!("no process may launch")
    }
}
#[test]
fn preexisting_receipt_and_cancellation_never_launch_or_overwrite() {
    let folder = tempfile::tempdir().unwrap();
    let output = folder.path().join("foreign");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("result.json"), b"foreign").unwrap();
    assert!(policy::run(
        &option(output.clone()),
        &mut RejectRun,
        &mut Vec::new(),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(fs::read(output.join("result.json")).unwrap(), b"foreign");
    let cancelled = folder.path().join("cancelled");
    assert!(policy::run(
        &option(cancelled.clone()),
        &mut RejectRun,
        &mut Vec::new(),
        &AtomicBool::new(true)
    )
    .is_err());
    assert!(!cancelled.join("result.json").exists());
}

struct OriginalShapeRunner {
    output: PathBuf,
    call: usize,
    corrupt: Option<&'static str>,
}
impl Runner for OriginalShapeRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        self.call += 1;
        if self.call == 1 {
            assert_eq!(command.timeout.as_secs(), 30);
            assert_eq!(command.stdout_limit, usize::MAX);
            assert_eq!(command.stderr_limit, usize::MAX);
            if self.corrupt == Some("timeout") {
                return Err(CaptureError::Timeout {
                    timeout: Duration::from_secs(30),
                    stdout: b"partial stdout".to_vec(),
                    stderr: b"partial stderr".to_vec(),
                }
                .into());
            }
            assert_eq!(
                command.input.as_ref().unwrap(),
                &fs::read(self.output.join("cases.jsonl"))?
            );
            let mut source: serde_json::Value =
                serde_json::from_slice(&fs::read(self.output.join("source.json"))?)?;
            if self.corrupt == Some("case") {
                source[0] = serde_json::json!({"accepted":true});
            }
            let mut rows = source
                .as_array()
                .unwrap()
                .iter()
                .map(serde_json::to_string)
                .collect::<std::result::Result<Vec<_>, _>>()?
                .join("\n")
                + "\n";
            if self.corrupt == Some("crlf") {
                rows = rows.replace('\n', "\r\n");
            }
            Ok(Captured {
                code: if self.corrupt == Some("exit") { 9 } else { 0 },
                stdout: rows.into_bytes(),
                stderr: if self.corrupt == Some("crlf") {
                    b"diagnostic\r\nsecond\rline".to_vec()
                } else {
                    vec![]
                },
            })
        } else {
            assert_eq!(command.timeout.as_secs(), 30);
            assert_eq!(command.stdout_limit, usize::MAX);
            assert_eq!(command.stderr_limit, usize::MAX);
            assert_eq!(command.effective_argv().len(), 2);
            let mut table = policy::digits()?;
            if self.corrupt == Some("digits") {
                table.pop();
            }
            let mut stdout = serde_json::to_vec(&table)?;
            if self.corrupt == Some("crlf") {
                stdout.extend_from_slice(b"\r\n");
            }
            Ok(Captured {
                code: 0,
                stdout,
                stderr: if self.corrupt == Some("crlf") {
                    b"digits\r\nsecond\rline".to_vec()
                } else {
                    vec![]
                },
            })
        }
    }
}

#[test]
fn subprocess_text_and_nonempty_stderr_match_original_capture_semantics() {
    let folder = tempfile::tempdir().unwrap();
    let output = folder.path().join("comparison");
    let mut runner = OriginalShapeRunner {
        output: output.clone(),
        call: 0,
        corrupt: Some("crlf"),
    };
    let report = policy::run(
        &option(output.clone()),
        &mut runner,
        &mut Vec::new(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(runner.call, 2);
    assert_eq!(report["cases"], 6363);
    assert!(output.join("result.json").exists());
    assert_eq!(
        fs::read(output.join("native.stderr")).unwrap(),
        b"diagnostic\nsecond\nline"
    );
    assert_eq!(
        fs::read(output.join("digits.stderr")).unwrap(),
        b"digits\r\nsecond\rline"
    );
    assert!(fs::read(output.join("native.jsonl"))
        .unwrap()
        .iter()
        .all(|byte| *byte != b'\r'));
    assert!(fs::read(output.join("digits.stdout"))
        .unwrap()
        .ends_with(b"\r\n"));
}
struct MutatingWriter {
    binary: PathBuf,
}
impl Write for MutatingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        fs::write(&self.binary, b"changed copied fixture")?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn copied_binary_changed_during_final_stdout_cannot_publish_receipt() {
    let folder = tempfile::tempdir().unwrap();
    let copy = folder.path().join("candidate-copy");
    fs::copy(
        root().join("rust/crates/bluetooth/examples/bluez_policy.rs"),
        &copy,
    )
    .unwrap();
    let output = folder.path().join("comparison");
    let mut options = option(output.clone());
    options.binary = copy.clone();
    let mut runner = OriginalShapeRunner {
        output: output.clone(),
        call: 0,
        corrupt: None,
    };
    assert!(policy::run(
        &options,
        &mut runner,
        &mut MutatingWriter { binary: copy },
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(runner.call, 2);
    assert!(!output.join("result.json").exists());
}

#[test]
fn one_wrong_response_or_missing_digit_rejects_receipt() {
    for corrupt in ["case", "digits", "timeout", "exit"] {
        let folder = tempfile::tempdir().unwrap();
        let output = folder.path().join("comparison");
        let mut runner = OriginalShapeRunner {
            output: output.clone(),
            call: 0,
            corrupt: Some(corrupt),
        };
        assert!(policy::run(
            &option(output.clone()),
            &mut runner,
            &mut Vec::new(),
            &AtomicBool::new(false),
        )
        .is_err());
        assert_eq!(runner.call, if corrupt == "digits" { 2 } else { 1 });
        if corrupt == "timeout" {
            assert_eq!(
                fs::read(output.join("native.jsonl")).unwrap(),
                b"partial stdout"
            );
            assert_eq!(
                fs::read(output.join("native.stderr")).unwrap(),
                b"partial stderr"
            );
        }
        assert!(!output.join("result.json").exists());
    }
}

#[cfg(target_os = "linux")]
#[path = "bluetooth_bluez_policy_check/lifecycle.rs"]
mod lifecycle;

#[test]
fn cli_keeps_help_and_required_path_contract() {
    let executable = env!("CARGO_BIN_EXE_check-bluetooth-bluez-policy");
    let help = Command::new(executable).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout)
        .unwrap()
        .contains("--binary BINARY --output OUTPUT"));
    let missing = Command::new(executable).output().unwrap();
    assert!(!missing.status.success());
}
