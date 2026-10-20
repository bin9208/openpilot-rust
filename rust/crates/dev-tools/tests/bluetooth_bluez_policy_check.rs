use openpilot_dev_tools::{
    bluetooth_bluez_policy_check::{self as policy, Options},
    qa_process::{CaptureError, Captured, CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::JsonView;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::symlink,
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
const HISTORICAL: &str = "rust/tools/check_bluetooth_bluez_policy.py";
fn private_root(present: bool) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for source in [
        "openpilot/selfdrive/carrot/bluetooth/bluez.py",
        "rust/crates/bluetooth/examples/bluez_policy.rs",
        "rust/crates/bluetooth/src/bluez/policy.rs",
    ] {
        let path = directory.path().join(source);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(root().join(source), path).unwrap();
    }
    let historical = directory.path().join(HISTORICAL);
    fs::create_dir_all(historical.parent().unwrap()).unwrap();
    if present {
        fs::copy(root().join(HISTORICAL), historical).unwrap();
    }
    directory
}
fn private_option(private: &Path, output: &Path) -> Options {
    Options {
        root: private.into(),
        binary: private.join("rust/crates/bluetooth/examples/bluez_policy.rs"),
        output: output.into(),
    }
}
fn shape(output: &Path) -> OriginalShapeRunner {
    OriginalShapeRunner {
        output: output.into(),
        call: 0,
        corrupt: None,
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
struct ReplaceWriter {
    path: PathBuf,
    replacement: &'static [u8],
}
impl Write for ReplaceWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        fs::write(&self.path, self.replacement)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct RemoveWriter(Option<PathBuf>);
impl Write for RemoveWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(path) = self.0.take() {
            fs::remove_file(path)?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
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

#[test]
fn present_and_absent_historical_checker_use_sealed_python_observation() {
    let observed_cases = include_bytes!(
        "fixtures/bluetooth-bluez-policy/original-observation-39499806776/cases.jsonl"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(observed_cases)),
        "db2d480a8bace324c8854bf837b1d2cc4270d2c058d481609de1532c4e0c9fbc"
    );
    let source_bytes = include_bytes!(
        "fixtures/bluetooth-bluez-policy/original-observation-39499806776/source.json"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(source_bytes)),
        "fedaa7902e27d97cc508b8acfcf0995edd6117ea55725fa3c658ebd54eb18e5d"
    );
    let receipt_bytes = include_bytes!(
        "fixtures/bluetooth-bluez-policy/original-observation-39499806776/result.json"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(receipt_bytes)),
        "923ec492db1924b994824dab7d09e505e39d0acd856cf049eecbc591ba11dbe5"
    );
    let observed_receipt: serde_json::Value = serde_json::from_slice(receipt_bytes).unwrap();
    let observed_source: serde_json::Value = serde_json::from_slice(source_bytes).unwrap();
    for present in [true, false] {
        let private = private_root(present);
        let output = private.path().join("comparison");
        let mut runner = shape(&output);
        let report = policy::run(
            &private_option(private.path(), &output),
            &mut runner,
            &mut Vec::new(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(runner.call, 2);
        assert_eq!(
            fs::read(output.join("cases.jsonl")).unwrap().as_slice(),
            observed_cases.as_slice()
        );
        let source: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("source.json")).unwrap()).unwrap();
        assert_eq!(source, observed_source);
        assert_eq!(report["cases"], 6363);
        assert_eq!(report["accepted"], 3598);
        for key in ["cases", "accepted", "digit_points", "unicode_version"] {
            assert_eq!(report[key], observed_receipt[key]);
        }
        assert!(output.join("result.json").exists());
    }
}

#[test]
fn historical_checker_admits_only_absence_or_exact_regular_content() {
    for defect in [
        "changed",
        "symlink",
        "fifo",
        "missing-parent",
        "missing-other",
    ] {
        let private = private_root(true);
        let historical = private.path().join(HISTORICAL);
        match defect {
            "changed" => fs::write(&historical, b"changed historical checker").unwrap(),
            "symlink" => {
                fs::remove_file(&historical).unwrap();
                symlink(root().join(HISTORICAL), &historical).unwrap();
            }
            "fifo" => {
                fs::remove_file(&historical).unwrap();
                rustix::fs::mkfifoat(
                    rustix::fs::CWD,
                    &historical,
                    rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                )
                .unwrap();
            }
            "missing-parent" => {
                fs::remove_file(&historical).unwrap();
                fs::remove_dir(historical.parent().unwrap()).unwrap();
            }
            "missing-other" => {
                fs::remove_file(
                    private
                        .path()
                        .join("openpilot/selfdrive/carrot/bluetooth/bluez.py"),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let output = private.path().join("comparison");
        assert!(
            policy::run(
                &private_option(private.path(), &output),
                &mut RejectRun,
                &mut Vec::new(),
                &AtomicBool::new(false),
            )
            .is_err(),
            "{defect}"
        );
        assert!(!output.join("result.json").exists(), "{defect}");
    }
}

#[test]
fn checker_mutation_or_reappearance_during_final_stdout_removes_owned_receipt() {
    for present in [true, false] {
        let private = private_root(present);
        let output = private.path().join("comparison");
        let mut runner = shape(&output);
        let mut writer = ReplaceWriter {
            path: private.path().join(HISTORICAL),
            replacement: if present {
                b"changed checker"
            } else {
                b"unexpected reappearance"
            },
        };
        let error = policy::run(
            &private_option(private.path(), &output),
            &mut runner,
            &mut writer,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(runner.call, 2);
        if !present {
            assert!(error.to_string().contains("presence changed"));
        }
        assert!(!output.join("result.json").exists());
    }
}

#[test]
fn historical_checker_disappearance_during_final_stdout_is_rejected() {
    let private = private_root(true);
    let output = private.path().join("comparison");
    let mut runner = shape(&output);
    let error = policy::run(
        &private_option(private.path(), &output),
        &mut runner,
        &mut RemoveWriter(Some(private.path().join(HISTORICAL))),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert_eq!(runner.call, 2);
    assert!(error.to_string().contains("presence changed"));
    assert!(!output.join("result.json").exists());
}

#[test]
fn absent_checker_parent_replacement_and_foreign_receipt_are_rejected() {
    let private = private_root(false);
    let output = private.path().join("comparison");
    let mut runner = shape(&output);
    struct ParentSwap<'a>(&'a Path);
    impl Write for ParentSwap<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let parent = self.0.join("rust/tools");
            fs::rename(&parent, self.0.join("rust/tools-old"))?;
            fs::create_dir(&parent)?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(policy::run(
        &private_option(private.path(), &output),
        &mut runner,
        &mut ParentSwap(private.path()),
        &AtomicBool::new(false),
    )
    .is_err());
    assert_eq!(runner.call, 2);
    assert!(!output.join("result.json").exists());

    let private = private_root(false);
    let output = private.path().join("foreign");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("result.json"), b"foreign").unwrap();
    assert!(policy::run(
        &private_option(private.path(), &output),
        &mut RejectRun,
        &mut Vec::new(),
        &AtomicBool::new(false),
    )
    .is_err());
    assert_eq!(fs::read(output.join("result.json")).unwrap(), b"foreign");
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
