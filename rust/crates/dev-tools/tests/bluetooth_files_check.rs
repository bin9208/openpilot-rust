#![cfg(feature = "bluetooth-files-native")]
use openpilot_dev_tools::{
    bluetooth_files_check::{self as checker, Options, CASES, SCALARS},
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn candidate() -> PathBuf {
    fs::canonicalize(
        std::env::var_os("BLUETOOTH_FILE_FIXTURE").expect("mandatory fresh file_fixture"),
    )
    .unwrap()
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Original {
    directory: tempfile::TempDir,
    files: Vec<(Vec<u8>, u32)>,
    input: Vec<u8>,
    booleans: Vec<bool>,
    report: Value,
}
fn original() -> Original {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("original-checker");
    let mut spec = CommandSpec::new_os(
        vec![
            std::env::var_os("BLUETOOTH_FILES_PYTHON")
                .expect("mandatory Python3.12 original oracle for CI"),
            repository()
                .join("rust/tools/check_bluetooth_files.py")
                .into_os_string(),
            "--binary".into(),
            candidate().into_os_string(),
            "--output".into(),
            output.as_os_str().into(),
        ],
        Duration::from_secs(60),
    );
    spec.cwd = Some(repository());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let observed = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        observed.code,
        0,
        "unchanged original failed: {}",
        String::from_utf8_lossy(&observed.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
    let files = (0..CASES)
        .map(|index| {
            let path = output.join("original").join(format!("{index}.json"));
            (
                fs::read(&path).unwrap(),
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
            )
        })
        .collect::<Vec<_>>();
    let booleans = files.iter().map(|(_, mode)| *mode == 0o600).collect();
    let input = fs::read(output.join("input.jsonl")).unwrap();
    // No native candidate observations supply these expected bytes or modes.
    let cases = checker::cases();
    assert_eq!(cases.len(), 15);
    assert_eq!(
        checker::payload(&cases),
        input,
        "full original JSONL input schedule"
    );
    for (index, (case, (bytes, mode))) in cases.iter().zip(&files).enumerate() {
        assert_eq!(&case.expected, bytes, "original full bytes {index}");
        assert_eq!(
            case.succeeded,
            *mode == 0o600,
            "original encoding outcome {index}"
        );
    }
    assert_eq!(fs::read_dir(output.join("original")).unwrap().count(), 15);
    Original {
        directory,
        files,
        input,
        booleans,
        report,
    }
}
fn options(root: &Path, binary: &Path, output: &Path) -> Options {
    Options {
        root: root.into(),
        binary: binary.into(),
        output: output.into(),
    }
}
#[test]
fn all_fifteen_source_bytes_modes_and_payload_match_unchanged_python_then_real_native() {
    let oracle = original();
    let output = oracle.directory.path().join("native-checker");
    let report = checker::check(
        &options(&repository(), &candidate(), &output),
        &mut NativeRunner,
        &mut vec![],
        &AtomicBool::new(false),
    )
    .unwrap();
    for key in [
        "cases",
        "all_unicode_scalar_values",
        "unicode_errors",
        "bytes",
        "comparison",
        "source_sha256",
        "binary_sha256",
    ] {
        assert_eq!(
            report[key], oracle.report[key],
            "full original summary {key}"
        );
    }
    assert_eq!(report["cases"], 15);
    assert_eq!(report["all_unicode_scalar_values"], SCALARS);
    assert_eq!(report["unicode_errors"], 3);
    for (index, (bytes, mode)) in oracle.files.iter().enumerate() {
        for side in ["original", "native"] {
            let path = output.join(side).join(format!("{index}.json"));
            assert_eq!(fs::read(&path).unwrap(), *bytes);
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                *mode
            );
        }
        assert_eq!(report["file_observations"][index]["sha256"], sha(bytes));
    }
    // Persist a compact actual original observation for parent review/sealing.
    let evidence = PathBuf::from(
        std::env::var_os("BLUETOOTH_FILES_EVIDENCE").expect("mandatory CI evidence output"),
    );
    fs::create_dir_all(&evidence).unwrap();
    fs::write(evidence.join("original-observation.json"),serde_json::to_vec_pretty(&json!({
        "kind":"actual unchanged Python checker source files, not native candidate answers",
        "source_checker_sha256":sha(&fs::read(repository().join("rust/tools/check_bluetooth_files.py")).unwrap()),
        "model_sha256":oracle.report["source_sha256"],"input_sha256":sha(&oracle.input),
        "files":oracle.files.iter().enumerate().map(|(i,(bytes,mode))|json!({"index":i,"sha256":sha(bytes),"bytes":bytes.len(),"mode":mode,"succeeded":*mode==0o600})).collect::<Vec<_>>()
    })).unwrap()).unwrap();
    fs::write(
        evidence.join("native-report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
struct Peer<'a> {
    oracle: &'a Original,
    mode: &'a str,
    source: PathBuf,
    alias: PathBuf,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::from_secs(30));
        assert_eq!(spec.input.as_ref().unwrap(), &self.oracle.input);
        let argv = spec.effective_argv();
        assert_eq!(argv.len(), 2);
        assert!(Path::new(&argv[0]).is_absolute());
        let output = Path::new(&argv[1]);
        fs::create_dir(output)?;
        for (index, (bytes, mode)) in self.oracle.files.iter().enumerate() {
            let path = output.join(format!("{index}.json"));
            fs::write(&path, bytes)?;
            fs::set_permissions(path, fs::Permissions::from_mode(*mode))?;
        }
        let mut lines = self
            .oracle
            .booleans
            .iter()
            .map(bool::to_string)
            .collect::<Vec<_>>();
        let mut code = 0;
        match self.mode {
            "numeric-true" => lines[0] = "1.0".into(), // Python list equality accepts True==1.0.
            "false" => lines[0] = "false".into(),
            "missing" => {
                lines.pop();
            }
            "extra" => lines.push("true".into()),
            "json" => lines[0] = "not JSON".into(),
            "exit" => code = 9,
            "bytes" => fs::write(output.join("0.json"), b"different")?,
            "mode" => {
                fs::set_permissions(output.join("0.json"), fs::Permissions::from_mode(0o644))?
            }
            "temporary" => fs::write(output.join("0.json.leftover.tmp"), b"unremoved")?,
            "source" => fs::write(&self.source, b"changed source")?,
            "executable" => fs::write(&self.alias, b"changed executable")?,
            "retarget" => {
                let other = self.alias.with_extension("other");
                fs::write(&other, b"same fake executable")?;
                fs::remove_file(&self.alias)?;
                symlink(other, &self.alias)?;
            }
            "timeout" => return Err("injected command timeout".into()),
            _ => {}
        }
        Ok(Captured {
            code,
            stdout: if self.mode == "decode-error" {
                vec![255]
            } else {
                format!("{}\n", lines.join("\n")).into_bytes()
            },
            stderr: b"retained diagnostic\n".to_vec(),
        })
    }
}
fn source_copy(root: &Path) {
    let pins: Value =
        serde_json::from_str(include_str!("fixtures/bluetooth-files/source-pins.json")).unwrap();
    for name in pins.as_object().unwrap().keys() {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(repository().join(name), path).unwrap();
    }
}
#[test]
fn original_sourced_peers_reject_result_file_identity_and_publication_failures() {
    let oracle = original();
    for mode in [
        "ok",
        "numeric-true",
        "false",
        "missing",
        "extra",
        "json",
        "decode-error",
        "exit",
        "bytes",
        "mode",
        "temporary",
        "source",
        "executable",
        "retarget",
        "timeout",
        "foreign-report",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        source_copy(&root);
        let executable = dir.path().join("target");
        fs::write(&executable, b"same fake executable").unwrap();
        let alias = dir.path().join("candidate");
        symlink(&executable, &alias).unwrap();
        let output = dir.path().join("output");
        if mode == "foreign-report" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("result.json"), b"foreign report").unwrap();
        }
        let mut peer = Peer {
            oracle: &oracle,
            mode,
            source: root.join("openpilot/selfdrive/carrot/bluetooth/model.py"),
            alias: alias.clone(),
            calls: 0,
        };
        let result = checker::check(
            &options(&root, &alias, &output),
            &mut peer,
            &mut vec![],
            &AtomicBool::new(false),
        );
        assert_eq!(
            result.is_ok(),
            matches!(mode, "ok" | "numeric-true"),
            "{mode}: {result:?}"
        );
        assert_eq!(peer.calls, 1);
        if mode == "foreign-report" {
            assert_eq!(
                fs::read(output.join("result.json")).unwrap(),
                b"foreign report"
            );
        } else {
            assert_eq!(
                output.join("result.json").exists(),
                result.is_ok(),
                "{mode}"
            );
        }
    }
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("binary");
    fs::write(&binary, b"owned identity").unwrap();
    let output = dir.path().join("output");
    let mut peer = Peer {
        oracle: &oracle,
        mode: "ok",
        source: PathBuf::new(),
        alias: binary.clone(),
        calls: 0,
    };
    assert!(checker::check(
        &options(&repository(), &binary, &output),
        &mut peer,
        &mut Broken,
        &AtomicBool::new(false)
    )
    .is_err());
    assert!(!output.join("result.json").exists());
    struct CancelOnFlush<'a>(&'a AtomicBool);
    impl Write for CancelOnFlush<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }
    let output = dir.path().join("cancel-at-publication");
    let stop = AtomicBool::new(false);
    assert!(checker::check(
        &options(&repository(), &binary, &output),
        &mut peer,
        &mut CancelOnFlush(&stop),
        &stop
    )
    .is_err());
    assert!(!output.join("result.json").exists());
}
#[cfg(target_os = "linux")]
#[test]
fn missing_changed_dangling_and_fifo_sources_or_executables_fail_without_execution() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("candidate executed before admission")
        }
    }
    for source in [true, false] {
        for kind in ["missing", "changed", "dangling", "fifo"] {
            if !source && kind == "changed" {
                continue;
            }
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("root");
            source_copy(&root);
            let binary = dir.path().join("binary");
            fs::write(&binary, b"owned identity").unwrap();
            let path = if source {
                root.join("openpilot/selfdrive/carrot/bluetooth/model.py")
            } else {
                binary.clone()
            };
            fs::remove_file(&path).unwrap();
            match kind {
                "changed" => fs::write(&path, b"changed").unwrap(),
                "dangling" => symlink(dir.path().join("absent"), &path).unwrap(),
                "fifo" => rustix::fs::mknodat(
                    rustix::fs::CWD,
                    &path,
                    rustix::fs::FileType::Fifo,
                    rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                    0,
                )
                .unwrap(),
                _ => {}
            }
            let output = dir.path().join("output");
            fs::create_dir(&output).unwrap();
            fs::write(output.join("result.json"), b"foreign report").unwrap();
            let result = checker::check(
                &options(&root, &binary, &output),
                &mut Never,
                &mut vec![],
                &AtomicBool::new(false),
            );
            assert!(result.is_err(), "source={source} kind={kind}");
            assert_eq!(
                fs::read(output.join("result.json")).unwrap(),
                b"foreign report"
            );
        }
    }
}
#[cfg(target_os = "linux")]
#[test]
fn actual_cli_preserves_raw_paths_and_argument_admission() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let checker = env!("CARGO_BIN_EXE_check-bluetooth-files");
    for args in [
        vec!["--help"],
        vec![],
        vec!["--unknown"],
        vec!["--binary", "value"],
    ] {
        let observed = NativeRunner
            .run(&CommandSpec::new(
                args.iter().fold(vec![checker.into()], |mut a, s| {
                    a.push((*s).into());
                    a
                }),
                Duration::from_secs(5),
            ))
            .unwrap();
        assert_eq!(observed.code, if args == ["--help"] { 0 } else { 2 });
    }
    let dir = tempfile::tempdir().unwrap();
    let binary = dir
        .path()
        .join(OsString::from_vec(b"candidate-\xff".to_vec()));
    symlink(candidate(), &binary).unwrap();
    let output = dir.path().join(OsString::from_vec(b"output-\xfe".to_vec()));
    let mut spec = CommandSpec::new_os(
        vec![
            checker.into(),
            "--binary".into(),
            binary.into_os_string(),
            "--output".into(),
            output.as_os_str().into(),
        ],
        Duration::from_secs(45),
    );
    spec.os_env = Some(
        [(
            std::ffi::OsString::from("OPENPILOT_RUST_ROOT"),
            repository().into_os_string(),
        )]
        .into_iter()
        .collect(),
    );
    let observed = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        observed.code,
        0,
        "{}",
        String::from_utf8_lossy(&observed.stderr)
    );
    let report: Value = serde_json::from_slice(&observed.stdout).unwrap();
    assert_eq!(report["cases"], 15);
    assert_eq!(report["unicode_errors"], 3);
    assert_eq!(
        report,
        serde_json::from_slice::<Value>(&fs::read(output.join("result.json")).unwrap()).unwrap()
    );
}
#[cfg(target_os = "linux")]
#[path = "bluetooth_files_check/lifecycle.rs"]
mod lifecycle;
