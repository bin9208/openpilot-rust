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
    // This manifest is byte-for-byte ORIGINAL Python evidence downloaded from
    // Actions. The reference encoder is separate from the candidate; authenticate
    // every reconstructed byte before using it in either positive or fake peers.
    let raw = include_bytes!("fixtures/bluetooth-files/original-observation.json");
    assert_eq!(
        sha(raw),
        "a70cdb6d3bf57a44e0d711149500759895c0af93032b6df922ab5d1917c01273"
    );
    let observed: Value = serde_json::from_slice(raw).unwrap();
    let provenance: Value =
        serde_json::from_str(include_str!("fixtures/bluetooth-files/provenance.json")).unwrap();
    assert_eq!(provenance["observation_sha256"], sha(raw));
    assert_eq!(
        provenance["source_commit"],
        "1364e48c96e888fa5d178c5eb2132d135cf30765"
    );
    assert_eq!(provenance["run_id"], 39190411681_u64);
    assert_eq!(provenance["job_id"], 117862093373_u64);
    assert_eq!(
        provenance["artifact_zip_sha256"],
        "52b48b41f8d2477f3e04b6f10cb0ca49472b86e761533d176cdf43c28e594636"
    );
    assert_eq!(
        observed["source_checker_sha256"],
        "e30b4cdca8c658cfbd713b6603ef23c64d21854ff41ffa1f611bdd43d2371682"
    );
    assert_eq!(
        observed["model_sha256"],
        "74f65767da6459358c8809f6e0adf465a4e7007b4bbf4e160b23c98819e6a633"
    );
    let cases = checker::cases();
    assert_eq!(cases.len(), CASES);
    let input = checker::payload(&cases);
    assert_eq!(observed["input_sha256"], sha(&input));
    assert_eq!(
        input.len() as u64,
        provenance["input_bytes"].as_u64().unwrap()
    );
    let slots = observed["files"].as_array().unwrap();
    assert_eq!(slots.len(), CASES);
    let mut files = vec![];
    let mut booleans = vec![];
    for (index, (case, slot)) in cases.into_iter().zip(slots).enumerate() {
        assert_eq!(slot["index"], index);
        assert_eq!(
            slot["sha256"],
            sha(&case.expected),
            "full original bytes {index}"
        );
        assert_eq!(slot["bytes"], case.expected.len());
        let succeeded = slot["succeeded"].as_bool().unwrap();
        let mode = u32::try_from(slot["mode"].as_u64().unwrap()).unwrap();
        assert_eq!(
            case.succeeded, succeeded,
            "original Unicode outcome {index}"
        );
        assert_eq!(mode, if succeeded { 0o600 } else { 0o644 });
        files.push((case.expected, mode));
        booleans.push(succeeded);
    }
    let report = json!({
        "cases":slots.len(), "all_unicode_scalar_values":1112064,
        "unicode_errors":booleans.iter().filter(|v| !**v).count(),
        "bytes":files.iter().map(|(v,_)|v.len()).sum::<usize>(),
        "comparison":"exact bytes and permissions", "source_sha256":observed["model_sha256"],
        // This is current executable identity, never a golden algorithm answer.
        "binary_sha256":sha(&fs::read(candidate()).unwrap())
    });
    Original {
        directory: tempfile::tempdir().unwrap(),
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
fn sealed_fifteen_original_observations_authenticate_reference_bytes_then_real_native() {
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
    // Copy the immutable source observation unchanged; never label this replay
    // as fresh Python execution or replace it with native candidate observations.
    let evidence = PathBuf::from(
        std::env::var_os("BLUETOOTH_FILES_EVIDENCE").expect("mandatory CI evidence output"),
    );
    fs::create_dir_all(&evidence).unwrap();
    fs::write(
        evidence.join("original-observation.json"),
        include_bytes!("fixtures/bluetooth-files/original-observation.json"),
    )
    .unwrap();
    fs::write(
        evidence.join("provenance.json"),
        include_bytes!("fixtures/bluetooth-files/provenance.json"),
    )
    .unwrap();
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
            "retired-reappeared" => {
                fs::create_dir_all(self.source.parent().unwrap())?;
                fs::write(&self.source, b"reintroduced checker")?;
            }
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
        if name == "rust/tools/check_bluetooth_files.py" {
            continue;
        }
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
        "retired-reappeared",
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
            source: root.join(if mode == "retired-reappeared" {
                "rust/tools/check_bluetooth_files.py"
            } else {
                "openpilot/selfdrive/carrot/bluetooth/model.py"
            }),
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
    for source in ["model", "fixture", "executable"] {
        for kind in ["missing", "changed", "dangling", "fifo"] {
            if source == "executable" && kind == "changed" {
                continue;
            }
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("root");
            source_copy(&root);
            let binary = dir.path().join("binary");
            fs::write(&binary, b"owned identity").unwrap();
            let path = match source {
                "model" => root.join("openpilot/selfdrive/carrot/bluetooth/model.py"),
                "fixture" => root.join("rust/crates/bluetooth/examples/file_fixture.rs"),
                _ => binary.clone(),
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
fn only_the_owned_historical_checker_may_be_absent_with_truthful_live_provenance() {
    let oracle = original();
    for kind in [
        "absent",
        "changed",
        "dangling",
        "symlink",
        "fifo",
        "directory",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("root");
        source_copy(&root);
        let retired = root.join("rust/tools/check_bluetooth_files.py");
        fs::create_dir_all(retired.parent().unwrap()).unwrap();
        match kind {
            "changed" => fs::write(&retired, b"changed historical checker").unwrap(),
            "dangling" => symlink(directory.path().join("absent-target"), &retired).unwrap(),
            "symlink" => {
                let target = directory.path().join("private-regular-target");
                fs::write(&target, b"private regular target").unwrap();
                symlink(target, &retired).unwrap();
            }
            "fifo" => rustix::fs::mknodat(
                rustix::fs::CWD,
                &retired,
                rustix::fs::FileType::Fifo,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                0,
            )
            .unwrap(),
            "directory" => fs::create_dir(&retired).unwrap(),
            _ => {}
        }
        let binary = directory.path().join("binary");
        fs::write(&binary, b"private fixture identity").unwrap();
        let output = directory.path().join("output");
        if kind != "absent" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("result.json"), b"foreign report").unwrap();
        }
        let mut peer = Peer {
            oracle: &oracle,
            mode: "ok",
            source: PathBuf::new(),
            alias: binary.clone(),
            calls: 0,
        };
        let result = checker::check(
            &options(&root, &binary, &output),
            &mut peer,
            &mut vec![],
            &AtomicBool::new(false),
        );
        assert_eq!(result.is_ok(), kind == "absent", "{kind}: {result:?}");
        assert_eq!(
            peer.calls,
            usize::from(kind == "absent"),
            "admission before invocation"
        );
        if kind == "absent" {
            let report = result.unwrap();
            assert_eq!(
                report["historical_source_sha256"]["rust/tools/check_bluetooth_files.py"],
                "e30b4cdca8c658cfbd713b6603ef23c64d21854ff41ffa1f611bdd43d2371682"
            );
            assert_eq!(
                report["reference_observation_sha256"],
                "a70cdb6d3bf57a44e0d711149500759895c0af93032b6df922ab5d1917c01273"
            );
            let inputs = report["input_sha256"].as_array().unwrap();
            assert_eq!(
                inputs.len(),
                4,
                "two live sources plus executable and alias"
            );
            assert!(!inputs.iter().any(|v| v["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("check_bluetooth_files.py"))));
            for path in [
                "openpilot/selfdrive/carrot/bluetooth/model.py",
                "rust/crates/bluetooth/examples/file_fixture.rs",
            ] {
                assert!(inputs
                    .iter()
                    .any(|v| v["path"] == root.join(path).to_str().unwrap()));
            }
        } else {
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
#[test]
fn fresh_post_retirement_cli_executes_only_sealed_native_checker_and_fixture() {
    use std::{collections::BTreeMap, ffi::OsString};

    let root = repository();
    assert!(
        matches!(
            fs::symlink_metadata(root.join("rust/tools/check_bluetooth_files.py")),
            Err(error) if error.kind() == io::ErrorKind::NotFound
        ),
        "the historical checker must be absent in this post-retirement gate"
    );
    let checker = fs::canonicalize(env!("CARGO_BIN_EXE_check-bluetooth-files")).unwrap();
    let fixture = candidate();
    let artifacts = BTreeMap::from([
        (
            checker.to_str().unwrap().to_owned(),
            sha(&fs::read(&checker).unwrap()),
        ),
        (
            fixture.to_str().unwrap().to_owned(),
            sha(&fs::read(&fixture).unwrap()),
        ),
    ]);
    assert_eq!(
        artifacts.len(),
        2,
        "the checker and fixture must be distinct"
    );
    let evidence = PathBuf::from(
        std::env::var_os("BLUETOOTH_FILES_EVIDENCE").expect("mandatory CI evidence output"),
    );
    fs::create_dir_all(&evidence).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("post-retirement-cli");
    let trace = evidence.join("native.exec");
    fs::write(
        evidence.join("artifacts.sha256"),
        artifacts
            .iter()
            .map(|(path, digest)| format!("{digest}  {path}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut spec = CommandSpec::new_os(
        vec![
            "/usr/bin/strace".into(),
            "-f".into(),
            "-qq".into(),
            "-s".into(),
            "65535".into(),
            "-e".into(),
            "trace=execve,execveat".into(),
            "-o".into(),
            trace.as_os_str().into(),
            checker.as_os_str().into(),
            "--binary".into(),
            fixture.as_os_str().into(),
            "--output".into(),
            output.as_os_str().into(),
        ],
        Duration::from_secs(60),
    );
    spec.set_os_env(BTreeMap::from([
        (OsString::from("PATH"), OsString::from("/nonexistent")),
        (
            OsString::from("OPENPILOT_RUST_ROOT"),
            root.as_os_str().into(),
        ),
    ]));
    spec.env_remove = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| {
            key.as_os_str() != std::ffi::OsStr::new("PATH")
                && key.as_os_str() != std::ffi::OsStr::new("OPENPILOT_RUST_ROOT")
        })
        .collect();
    let stdout = evidence.join("native.stdout");
    let stderr = evidence.join("native.stderr");
    let observed = NativeRunner.run_to_files(&spec, &stdout, &stderr);
    if let Ok(bytes) = fs::read(output.join("result.json")) {
        fs::write(evidence.join("native-result.json"), bytes).unwrap();
    }
    let observed = observed.unwrap();
    assert!(
        observed.code == 0,
        "{}",
        String::from_utf8_lossy(&fs::read(&stderr).unwrap())
    );
    let report: Value = serde_json::from_slice(&fs::read(&stdout).unwrap()).unwrap();
    assert_eq!(report["cases"], 15);
    assert_eq!(report["unicode_errors"], 3);
    assert_eq!(
        report,
        serde_json::from_slice::<Value>(&fs::read(output.join("result.json")).unwrap()).unwrap()
    );
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let executable = regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*")"#).unwrap();
    let mut attempts = Vec::new();
    for line in fs::read_to_string(&trace).unwrap().lines() {
        if !opening.is_match(line) {
            continue;
        }
        let capture = executable
            .captures(line)
            .unwrap_or_else(|| panic!("opaque/unaccounted execve or execveat attempt: {line}"));
        let path: String = serde_json::from_str(&capture[1]).unwrap();
        assert!(
            artifacts.contains_key(&path),
            "unsealed executable attempt: {line}"
        );
        attempts.push(path);
    }
    assert_eq!(
        attempts.len(),
        2,
        "exactly the sealed checker and fixture execute"
    );
    assert_eq!(attempts[0], checker.to_str().unwrap());
    assert_eq!(attempts[1], fixture.to_str().unwrap());
    for (path, expected) in &artifacts {
        assert_eq!(sha(&fs::read(path).unwrap()), *expected);
    }
    fs::write(
        evidence.join("native-exec-audit.json"),
        serde_json::to_vec_pretty(&json!({
            "passed": true,
            "source_retired": true,
            "cases": 15,
            "exec_attempts": attempts,
            "sealed_executables_sha256": artifacts,
            "path": "/nonexistent"
        }))
        .unwrap(),
    )
    .unwrap();
}
#[cfg(target_os = "linux")]
#[path = "bluetooth_files_check/lifecycle.rs"]
mod lifecycle;
