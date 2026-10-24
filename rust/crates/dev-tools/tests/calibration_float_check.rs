//! Sealed original observations plus controlled process-boundary failures after Python retirement.
#![cfg(all(unix, feature = "calibration-float-native"))]
use openpilot_dev_tools::{
    calibration_float_check as checker,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    os::unix::{ffi::OsStringExt, fs::symlink},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn binary() -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os("CALIBRATION_FLOAT_BINARY")
            .expect("CALIBRATION_FLOAT_BINARY must name the current calibration_float example"),
    );
    assert!(
        path.is_absolute() && path.is_file(),
        "fresh calibration_float ELF prerequisite missing"
    );
    path
}
fn options(binary: PathBuf, output: PathBuf) -> checker::Options {
    checker::Options {
        root: root(),
        binary,
        output,
    }
}
fn value(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn require_retired() {
    assert!(
        matches!(fs::symlink_metadata(root().join(checker::CHECKER)),
        Err(error) if error.kind() == io::ErrorKind::NotFound)
    );
}
fn sealed_original_report() -> Value {
    let bytes = include_bytes!("fixtures/calibration-float-original-7b5/report.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "c196cadfd49fa7ce417b4987ec2c03ce1d048d3d9c686fca1ecc0aebef24e6e2"
    );
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn native_retains_all_25_sealed_original_results_and_generated_source() {
    let directory = tempfile::tempdir().unwrap();
    require_retired();
    let native = directory.path().join("native");
    let mut stdout = Vec::new();
    let report = checker::check(
        &options(binary(), native.clone()),
        &mut NativeRunner,
        &mut stdout,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(report, sealed_original_report());
    assert_eq!(report, value(&native.join("report.json")));
    assert_eq!(report["cases"].as_array().unwrap().len(), 25);
    assert_eq!(stdout, checker::SUCCESS.as_bytes());
    let cpp = include_bytes!("fixtures/calibration-float-original-7b5/source_get_float.cc");
    assert_eq!(
        format!("{:x}", Sha256::digest(cpp)),
        "71e73caa64a4dcc851edc57dc4167b38b01758ba9e16159e5a6a24fe430ce803"
    );
    assert_eq!(fs::read(native.join("source_get_float.cc")).unwrap(), cpp);
    require_retired();
    let provenance = value(&native.join("provenance.json"));
    assert_eq!(
        provenance["sources"]["source_sha256"][checker::HEADER],
        checker::HEADER_SHA256
    );
    assert!(provenance["sources"]["source_sha256"]
        .get(checker::CHECKER)
        .is_none());
    assert_eq!(
        provenance["sources"]["historical_source_sha256"][checker::CHECKER],
        checker::CHECKER_SHA256
    );
    assert_eq!(
        provenance["binary_sha256"],
        format!("{:x}", Sha256::digest(fs::read(binary()).unwrap()))
    );
    for key in [
        "compiler_sha256",
        "reference_sha256",
        "executing_checker_sha256",
        "generated_source_sha256",
    ] {
        assert_eq!(provenance[key].as_str().unwrap().len(), 64);
    }
}

#[test]
fn independent_full_raw_input_roster_is_not_shortened_or_line_encoded() {
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/calibration-float/inputs.json")).unwrap();
    let actual: Vec<&str> = checker::inputs()
        .iter()
        .map(|v| std::str::from_utf8(v).unwrap())
        .collect();
    assert_eq!(json!(actual), expected);
    assert_eq!(checker::inputs().len(), 25);
    assert_eq!(checker::inputs()[14], b"1\0garbage");
    assert_eq!(checker::inputs()[15], b"\0");
}

#[test]
fn exact_float_comparison_preserves_nan_exception_and_signed_zero_rules() {
    for (source, native) in [
        ("nan", "-NaN"),
        ("-0", "-0.0"),
        ("0", "0.0"),
        ("inf", "+Infinity"),
        ("ERROR", "ERROR"),
        ("1.25", "1_._25"),
    ] {
        assert_eq!(checker::compare(source, native).is_ok(), native != "1_._25");
    }
    for (source, native) in [
        ("-0", "0"),
        ("0", "-0"),
        ("ERROR", "nan"),
        ("1", "ERROR"),
        ("nan", "0"),
        ("1.0000001192092896", "1"),
        ("-inf", "inf"),
        ("1", "nan(payload)"),
    ] {
        assert!(
            checker::compare(source, native).is_err(),
            "{source} vs {native}"
        );
    }
    assert!(checker::compare("12.5", "１_２.５").is_ok());
}

#[test]
fn extraction_uses_actual_method_and_rejects_missing_or_unterminated_source() {
    let header = fs::read_to_string(root().join(checker::HEADER)).unwrap();
    let generated = checker::source_program(&header).unwrap();
    assert!(generated.contains("return value.empty() ? 0.0 : std::stof(value);"));
    assert!(generated.contains("std::setprecision(17)"));
    assert!(!generated.contains("inline int getInt"));
    assert!(checker::source_program("").is_err());
    assert!(checker::source_program("  inline float getFloat(").is_err());
    let changed = header.replace("std::stof(value)", "std::stof(value) + 1");
    assert!(checker::source_program(&changed)
        .unwrap()
        .contains("std::stof(value) + 1"));
}

#[test]
fn original_flags_abbreviations_repeated_options_and_raw_paths_are_admitted() {
    assert!(checker::parse(&["--help".into()]).unwrap().is_none());
    assert!(checker::parse(&["-hfoo".into()]).unwrap().is_none());
    assert!(checker::parse(&["-h=foo".into()]).is_err());
    assert!(checker::parse(&["-h".into(), "-hfoo".into()])
        .unwrap()
        .is_none());
    for args in [
        vec![],
        vec!["--binary", "x"],
        vec!["--output", "y"],
        vec!["--binary", "x", "--output", "y", "extra"],
        vec!["--binary"],
        vec!["--unknown"],
    ] {
        assert!(checker::parse(&args.into_iter().map(OsString::from).collect::<Vec<_>>()).is_err());
    }
    let raw = OsString::from_vec(b"raw-\xff".to_vec());
    let args = [
        "--bin".into(),
        "unused".into(),
        "--binary".into(),
        raw.clone(),
        "--out".into(),
        "output".into(),
    ];
    let parsed = checker::parse(&args).unwrap().unwrap();
    assert!(parsed.binary.ends_with(Path::new(&raw)));
    assert!(parsed.output.ends_with("output"));
}

#[test]
fn native_retains_twenty_original_help_and_error_admission_cases() {
    require_retired();
    for arguments in [
        vec![],
        vec!["--binary", "x"],
        vec!["--output", "y"],
        vec!["--help"],
        vec!["-h"],
        vec!["--hel"],
        vec!["-hfoo"],
        vec!["-hh"],
        vec!["-hhh"],
        vec!["-h-h"],
        vec!["-hh=foo"],
        vec!["-h=foo"],
        vec!["--help=foo"],
        vec!["--binary"],
        vec!["--binary", "--output", "x"],
        vec!["--unknown"],
        vec!["--binary", "x", "--output", "y", "extra"],
        vec!["--binary=x", "--output=y", "--unknown"],
        vec!["--help", "--unknown"],
        vec!["-h", "-hfoo"],
    ] {
        let mut native = vec![env!("CARGO_BIN_EXE_check-calibration-float").into()];
        native.extend(arguments.iter().map(OsString::from));
        let native = NativeRunner
            .run(&CommandSpec::new_os(native, Duration::from_secs(10)))
            .unwrap();
        let historical_code = if arguments
            .iter()
            .any(|arg| ["--help", "-h", "--hel", "-hfoo", "-hh", "-hhh"].contains(arg))
            && !arguments
                .iter()
                .any(|arg| ["-h-h", "-hh=foo", "-h=foo", "--help=foo"].contains(arg))
        {
            // The fresh original admission for this full roster passed before
            // deletion; the twelve raw original CLI receipts are separately
            // pinned by calibration_float_postdelete.
            0
        } else {
            2
        };
        assert_eq!(native.code, historical_code, "{arguments:?}");
        assert!(matches!(native.code, 0 | 2), "must not invoke workload");
        if native.code == 0 {
            assert!(!native.stdout.is_empty());
            assert!(native.stderr.is_empty());
        } else {
            assert!(native.stdout.is_empty());
            assert!(!native.stderr.is_empty());
        }
    }
}

struct Never;
impl Runner for Never {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        panic!("must not spawn")
    }
}
#[test]
fn foreign_existing_output_and_cancelled_admission_do_not_touch_a_success_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("foreign");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("report.json"), b"foreign pass sentinel").unwrap();
    let opts = checker::Options {
        root: directory.path().join("missing-root"),
        binary: PathBuf::from("missing"),
        output: output.clone(),
    };
    assert!(checker::check(&opts, &mut Never, &mut vec![], &AtomicBool::new(true)).is_err());
    assert_eq!(
        fs::read(output.join("report.json")).unwrap(),
        b"foreign pass sentinel"
    );
    let opts = checker::Options {
        output: directory.path().join("owned"),
        ..opts
    };
    let error = checker::check(&opts, &mut Never, &mut vec![], &AtomicBool::new(true)).unwrap_err();
    assert_eq!(
        error.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!opts.output.join("report.json").exists());
}

fn copy_sources(root_copy: &Path) {
    let target = root_copy.join(checker::HEADER);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(root().join(checker::HEADER), target).unwrap();
    fs::create_dir_all(root_copy.join(checker::CHECKER).parent().unwrap()).unwrap();
}
#[test]
fn source_guard_rejects_drift_and_dangling_objects_and_splits_genuine_retirement() {
    let directory = tempfile::tempdir().unwrap();
    copy_sources(directory.path());
    assert!(checker::sources(directory.path()).is_ok());
    let checker_path = directory.path().join(checker::CHECKER);
    let checker_copy = directory.path().join("checker-copy");
    assert!(!checker_path.exists());
    fs::write(&checker_copy, b"private unexpected reappearance").unwrap();
    symlink(&checker_copy, &checker_path).unwrap();
    assert!(checker::sources(directory.path()).is_err());
    fs::remove_file(&checker_path).unwrap();
    fs::copy(&checker_copy, &checker_path).unwrap();
    fs::write(&checker_path, b"changed").unwrap();
    assert!(checker::sources(directory.path()).is_err());
    fs::remove_file(&checker_path).unwrap();
    symlink("missing-target", &checker_path).unwrap();
    assert!(checker::sources(directory.path()).is_err());
    fs::remove_file(&checker_path).unwrap();
    let proof = checker::sources(directory.path()).unwrap();
    assert!(proof["source_sha256"].get(checker::CHECKER).is_none());
    assert_eq!(
        proof["historical_source_sha256"][checker::CHECKER],
        checker::CHECKER_SHA256
    );
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &checker_path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    assert!(checker::sources(directory.path()).is_err());
    fs::remove_file(&checker_path).unwrap();
    fs::remove_dir(checker_path.parent().unwrap()).unwrap();
    assert!(checker::sources(directory.path()).is_err());
    let other_tools = directory.path().join("other-tools");
    fs::create_dir(&other_tools).unwrap();
    symlink(&other_tools, checker_path.parent().unwrap()).unwrap();
    assert!(checker::sources(directory.path()).is_err());
    fs::remove_file(checker_path.parent().unwrap()).unwrap();
    fs::create_dir(checker_path.parent().unwrap()).unwrap();
    let header_path = directory.path().join(checker::HEADER);
    let header_copy = directory.path().join("header-copy");
    fs::copy(&header_path, &header_copy).unwrap();
    fs::remove_file(&header_path).unwrap();
    symlink(&header_copy, &header_path).unwrap();
    assert!(checker::sources(directory.path()).is_err());
    fs::remove_file(&header_path).unwrap();
    assert!(checker::sources(directory.path()).is_err());
}

struct Boundary<'a> {
    calls: usize,
    options: &'a checker::Options,
    mutation: &'a str,
    cancel: &'a AtomicBool,
}
impl Runner for Boundary<'_> {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        let index = self.calls;
        self.calls += 1;
        if index == 0 {
            assert_eq!(command.timeout, Duration::from_secs(60));
            assert_eq!(command.effective_argv()[1], "-std=c++17");
            if self.mutation == "compiler-failure" {
                return Ok(Captured {
                    code: 17,
                    stdout: b"compiler output".to_vec(),
                    stderr: b"compile failed".to_vec(),
                });
            }
            fs::write(
                self.options.output.join("source-get-float"),
                b"controlled nonexecuted reference identity",
            )
            .unwrap();
        } else {
            assert_eq!(command.timeout, Duration::from_secs(5));
            assert_eq!(
                command.input.as_deref(),
                Some(checker::inputs()[(index - 1) / 2])
            );
            if index == 2 {
                match self.mutation {
                    "replace-candidate" => {
                        fs::rename(
                            &self.options.binary,
                            self.options.binary.with_extension("old"),
                        )
                        .unwrap();
                        fs::write(&self.options.binary, b"candidate identity").unwrap();
                    }
                    "source-drift" => {
                        fs::write(
                            self.options.root.join(checker::HEADER),
                            b"changed while running",
                        )
                        .unwrap();
                    }
                    "source-identity-change" => {
                        let source = self.options.root.join(checker::CHECKER);
                        if source.exists() {
                            fs::remove_file(&source).unwrap();
                        } else {
                            fs::write(&source, b"unexpected checker reappearance").unwrap();
                        }
                    }
                    "cancel" => self.cancel.store(true, Ordering::Relaxed),
                    "candidate-failure" => {
                        return Ok(Captured {
                            code: 19,
                            stdout: vec![],
                            stderr: b"candidate failed".to_vec(),
                        })
                    }
                    "mismatch" => {
                        return Ok(Captured {
                            code: 0,
                            stdout: b"-0".to_vec(),
                            stderr: vec![],
                        })
                    }
                    _ => {}
                }
            }
        }
        Ok(Captured {
            code: 0,
            stdout: if index == 0 { vec![] } else { b"0".to_vec() },
            stderr: vec![],
        })
    }
}
struct Closed;
impl Write for Closed {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
}
#[test]
fn controlled_failures_and_postexecution_identity_changes_never_publish_success() {
    for mutation in [
        "compiler-failure",
        "candidate-failure",
        "mismatch",
        "replace-candidate",
        "source-drift",
        "source-identity-change",
        "cancel",
        "closed-stdout",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        copy_sources(&source);
        let candidate = directory.path().join("candidate");
        fs::write(&candidate, b"candidate identity").unwrap();
        let opts = checker::Options {
            root: source,
            binary: candidate,
            output: directory.path().join("result"),
        };
        let cancel = AtomicBool::new(false);
        let mut runner = Boundary {
            calls: 0,
            options: &opts,
            mutation,
            cancel: &cancel,
        };
        let mut sink = Vec::new();
        let mut closed = Closed;
        let stdout: &mut dyn Write = if mutation == "closed-stdout" {
            &mut closed
        } else {
            &mut sink
        };
        assert!(
            checker::check(&opts, &mut runner, stdout, &cancel).is_err(),
            "{mutation}"
        );
        assert!(!opts.output.join("report.json").exists(), "{mutation}");
        assert!(!opts.output.join("provenance.json").exists(), "{mutation}");
        if mutation == "compiler-failure" {
            assert_eq!(
                fs::read(opts.output.join("compiler.stderr")).unwrap(),
                b"compile failed"
            );
        }
        if mutation == "candidate-failure" {
            assert_eq!(
                fs::read(opts.output.join("00-native.stderr")).unwrap(),
                b"candidate failed"
            );
        }
    }
}

struct CancelOnFlush<'a>(&'a AtomicBool);
impl Write for CancelOnFlush<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.store(true, Ordering::Relaxed);
        Ok(())
    }
}
#[test]
fn cancellation_between_final_stdout_and_publication_rolls_back_both_owned_reports() {
    let directory = tempfile::tempdir().unwrap();
    copy_sources(directory.path());
    let candidate = directory.path().join("candidate");
    fs::write(&candidate, b"candidate identity").unwrap();
    let opts = checker::Options {
        root: directory.path().into(),
        binary: candidate,
        output: directory.path().join("result"),
    };
    let cancel = AtomicBool::new(false);
    let mut runner = Boundary {
        calls: 0,
        options: &opts,
        mutation: "",
        cancel: &cancel,
    };
    let error =
        checker::check(&opts, &mut runner, &mut CancelOnFlush(&cancel), &cancel).unwrap_err();
    assert_eq!(
        error.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(runner.calls, 51);
    assert!(!opts.output.join("report.json").exists());
    assert!(!opts.output.join("provenance.json").exists());
}

#[test]
fn actual_native_cli_runs_on_lossless_paths_and_rejects_a_failing_probe() {
    let directory = tempfile::tempdir().unwrap();
    let candidate = directory
        .path()
        .join(OsString::from_vec(b"candidate-\xff".to_vec()));
    // A copied regular ELF keeps the raw filename after Path.resolve(), unlike
    // a symlink whose target legitimately resolves to the original UTF-8 path.
    fs::copy(binary(), &candidate).unwrap();
    let output = directory
        .path()
        .join(OsString::from_vec(b"output-\xfe".to_vec()));
    let mut spec = CommandSpec::new_os(
        vec![
            env!("CARGO_BIN_EXE_check-calibration-float").into(),
            "--binary".into(),
            candidate.clone().into_os_string(),
            "--output".into(),
            output.clone().into_os_string(),
        ],
        Duration::from_secs(180),
    );
    spec.env.insert(
        "OPENPILOT_RUST_ROOT".into(),
        root().to_str().unwrap().into(),
    );
    let actual = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        actual.code,
        0,
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert_eq!(actual.stdout, checker::SUCCESS.as_bytes());
    let provenance = value(&output.join("provenance.json"));
    use std::os::unix::ffi::OsStrExt;
    assert_eq!(
        provenance["binary_path_bytes"],
        json!(candidate.as_os_str().as_bytes())
    );
    let failed_output = directory.path().join("failed");
    spec.os_argv = Some(vec![
        env!("CARGO_BIN_EXE_check-calibration-float").into(),
        "--binary".into(),
        "/bin/false".into(),
        "--output".into(),
        failed_output.clone().into_os_string(),
    ]);
    assert_eq!(NativeRunner.run(&spec).unwrap().code, 1);
    assert!(!failed_output.join("report.json").exists());
}

#[test]
fn native_retains_original_unset_path_and_explicitly_empty_path_failures() {
    let directory = tempfile::tempdir().unwrap();
    require_retired();
    let receipt = include_bytes!("fixtures/calibration-float-original-7b5/cli-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(receipt)),
        "48504f07bbae0dc0ae768b0d6d132a42bf148808a93d92c901f73b5f7aadb84f"
    );
    let receipt: Value = serde_json::from_slice(receipt).unwrap();
    for unset in [true, false] {
        let mut outcomes = Vec::new();
        {
            let output = directory.path().join(format!("path-{unset}"));
            let mut argv = vec![env!("CARGO_BIN_EXE_check-calibration-float").into()];
            argv.extend([
                "--binary".into(),
                binary().into_os_string(),
                "--output".into(),
                output.clone().into_os_string(),
            ]);
            let mut command = CommandSpec::new_os(argv, Duration::from_secs(180));
            // A bare argv[0] can affect GCC helper lookup even when its driver
            // resolves through the default PATH. Compare the actual original
            // outcome instead of assuming that this host's compiler succeeds.
            command.cwd = Some(directory.path().into());
            command.env.insert(
                "OPENPILOT_RUST_ROOT".into(),
                root().to_str().unwrap().into(),
            );
            if unset {
                command.env_remove.push("PATH".into());
            } else {
                command.env.insert("PATH".into(), String::new());
            }
            let observed = NativeRunner.run(&command).unwrap();
            assert!(
                matches!(observed.code, 0 | 1),
                "unset={unset}: {}",
                String::from_utf8_lossy(&observed.stderr)
            );
            if !unset {
                assert_eq!(observed.code, 1, "empty PATH must not find the compiler");
            }
            assert!(output.join("source_get_float.cc").is_file());
            let report = if observed.code == 0 {
                assert_eq!(observed.stdout, checker::SUCCESS.as_bytes());
                Some(value(&output.join("report.json")))
            } else {
                assert!(!output.join("report.json").exists());
                assert!(!output.join("provenance.json").exists());
                None
            };
            outcomes.push((observed.code, observed.stdout, report));
        }
        {
            let name = if unset { "path-unset" } else { "path-empty" };
            let original = receipt["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["name"] == name)
                .unwrap();
            assert_eq!(
                outcomes[0].0,
                original["code"].as_i64().unwrap() as i32,
                "{name}"
            );
            assert_eq!(
                outcomes[0].2.is_some(),
                original["report_present"].as_bool().unwrap(),
                "{name}"
            );
            assert_eq!(
                outcomes[0].1.is_empty(),
                original["stdout_hex"] == "",
                "{name}"
            );
        }
    }
}
