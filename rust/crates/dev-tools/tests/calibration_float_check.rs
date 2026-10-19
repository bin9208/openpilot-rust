//! Cloud-only real original comparison plus controlled process-boundary failures.
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

#[test]
fn unchanged_checker_in_cloud_and_native_have_all_25_identical_results_and_generated_source() {
    let directory = tempfile::tempdir().unwrap();
    let python = std::env::var_os("CALIBRATION_FLOAT_PYTHON")
        .expect("CALIBRATION_FLOAT_PYTHON must name the cloud original-control interpreter");
    let original = directory.path().join("original");
    let native = directory.path().join("native");
    let original_bytes =
        fs::read(root().join(checker::CHECKER)).expect("original must remain for cloud comparison");
    assert_eq!(
        format!("{:x}", Sha256::digest(&original_bytes)),
        checker::CHECKER_SHA256
    );
    let result = NativeRunner
        .run(&CommandSpec::new_os(
            vec![
                python,
                root().join(checker::CHECKER).into_os_string(),
                "--binary".into(),
                binary().into_os_string(),
                "--output".into(),
                original.clone().into_os_string(),
            ],
            Duration::from_secs(180),
        ))
        .unwrap();
    assert_eq!(
        result.code,
        0,
        "original stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let mut stdout = Vec::new();
    let report = checker::check(
        &options(binary(), native.clone()),
        &mut NativeRunner,
        &mut stdout,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(report, value(&original.join("report.json")));
    assert_eq!(report, value(&native.join("report.json")));
    assert_eq!(report["cases"].as_array().unwrap().len(), 25);
    assert_eq!(stdout, result.stdout);
    assert_eq!(stdout, checker::SUCCESS.as_bytes());
    assert_eq!(
        fs::read(native.join("source_get_float.cc")).unwrap(),
        fs::read(original.join("source_get_float.cc")).unwrap()
    );
    assert_eq!(
        fs::read(root().join(checker::CHECKER)).unwrap(),
        original_bytes
    );
    let provenance = value(&native.join("provenance.json"));
    assert_eq!(
        provenance["sources"]["source_sha256"][checker::HEADER],
        checker::HEADER_SHA256
    );
    assert_eq!(
        provenance["sources"]["source_sha256"][checker::CHECKER],
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
fn cloud_original_and_native_cli_have_the_same_help_and_error_admission() {
    let python = std::env::var_os("CALIBRATION_FLOAT_PYTHON")
        .expect("cloud original-control interpreter is mandatory");
    for arguments in [
        vec![],
        vec!["--binary", "x"],
        vec!["--output", "y"],
        vec!["--help"],
        vec!["-h"],
        vec!["--hel"],
        vec!["-hfoo"],
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
        let mut source = vec![
            python.clone(),
            root().join(checker::CHECKER).into_os_string(),
        ];
        source.extend(arguments.iter().map(OsString::from));
        let mut native = vec![env!("CARGO_BIN_EXE_check-calibration-float").into()];
        native.extend(arguments.iter().map(OsString::from));
        let source = NativeRunner
            .run(&CommandSpec::new_os(source, Duration::from_secs(10)))
            .unwrap();
        let native = NativeRunner
            .run(&CommandSpec::new_os(native, Duration::from_secs(10)))
            .unwrap();
        assert_eq!(native.code, source.code, "{arguments:?}");
        assert!(
            source.code == 0 || source.code == 2,
            "must not invoke the workload: {arguments:?}"
        );
        if source.code == 0 {
            assert!(!native.stdout.is_empty());
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
    for path in [checker::HEADER, checker::CHECKER] {
        let target = root_copy.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(path), target).unwrap();
    }
}
#[test]
fn source_guard_rejects_drift_and_dangling_objects_and_splits_genuine_retirement() {
    let directory = tempfile::tempdir().unwrap();
    copy_sources(directory.path());
    assert!(checker::sources(directory.path()).is_ok());
    let checker_path = directory.path().join(checker::CHECKER);
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
    fs::remove_file(directory.path().join(checker::HEADER)).unwrap();
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
                    "source-absence" => {
                        fs::remove_file(self.options.root.join(checker::CHECKER)).unwrap();
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
        "source-absence",
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
fn cloud_original_and_native_distinguish_unset_path_from_explicitly_empty_path() {
    let directory = tempfile::tempdir().unwrap();
    let python = fs::canonicalize(PathBuf::from(
        std::env::var_os("CALIBRATION_FLOAT_PYTHON")
            .expect("cloud original-control interpreter is mandatory"),
    ))
    .unwrap();
    for unset in [true, false] {
        let mut reports = Vec::new();
        for original in [true, false] {
            let output = directory.path().join(format!("path-{unset}-{original}"));
            let mut argv = if original {
                vec![
                    python.clone().into_os_string(),
                    root().join(checker::CHECKER).into_os_string(),
                ]
            } else {
                vec![env!("CARGO_BIN_EXE_check-calibration-float").into()]
            };
            argv.extend([
                "--binary".into(),
                binary().into_os_string(),
                "--output".into(),
                output.clone().into_os_string(),
            ]);
            let mut command = CommandSpec::new_os(argv, Duration::from_secs(180));
            // There is deliberately no g++ in this private cwd. No process-wide
            // environment/cwd mutation can interfere with concurrent tests.
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
            assert_eq!(
                observed.code,
                if unset { 0 } else { 1 },
                "unset={unset}, original={original}: {}",
                String::from_utf8_lossy(&observed.stderr)
            );
            assert!(output.join("source_get_float.cc").is_file());
            if unset {
                assert_eq!(observed.stdout, checker::SUCCESS.as_bytes());
                reports.push(value(&output.join("report.json")));
            } else {
                assert!(!output.join("report.json").exists());
            }
        }
        if unset {
            assert_eq!(reports[0], reports[1]);
        }
    }
}
