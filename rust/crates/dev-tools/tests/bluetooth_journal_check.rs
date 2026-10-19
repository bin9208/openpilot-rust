#![cfg(feature = "bluetooth-journal-native")]
use openpilot_dev_tools::{
    bluetooth_journal_check::{self as checker, Options, OPERATIONS},
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn candidate() -> PathBuf {
    fs::canonicalize(
        std::env::var_os("BLUETOOTH_JOURNAL_FIXTURE").expect("mandatory fresh journal_fixture"),
    )
    .unwrap()
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[test]
fn public_trace_parser_keeps_python_crlf_and_other_line_boundaries() {
    for separator in [
        "\r\n", "\n", "\r", "\u{b}", "\u{c}", "\u{1c}", "\u{1d}", "\u{1e}", "\u{85}", "\u{2028}",
        "\u{2029}",
    ] {
        let rows = checker::parse_trace(&format!("[1]{separator}[2]{separator}")).unwrap();
        assert_eq!(rows.len(), 2, "{separator:?}");
        assert_eq!(rows[0].to_json().unwrap(), "[1]");
        assert_eq!(rows[1].to_json().unwrap(), "[2]");
    }
    assert!(checker::parse_trace("").unwrap().is_empty());
    // Only CRLF is paired. A genuine blank line still reaches json.loads and
    // fails; this fix must not silently discard empty records.
    for text in ["[]\r\n\r\n[]", "[]\r\r\n[]", "[]\n\n[]"] {
        assert!(checker::parse_trace(text).is_err(), "{text:?}");
    }
}
struct Original {
    directory: tempfile::TempDir,
    rows: Vec<JsonValue>,
    input: Vec<u8>,
    report: Value,
}
fn original() -> Original {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("source-run");
    let mut spec = CommandSpec::new_os(
        vec![
            std::env::var_os("BLUETOOTH_JOURNAL_PYTHON")
                .expect("mandatory Python3.12 unchanged CI oracle"),
            repository()
                .join("rust/tools/check_bluetooth_journal.py")
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
    let input = fs::read(output.join("input.jsonl")).unwrap();
    let rows =
        checker::parse_trace(&fs::read_to_string(output.join("original.jsonl")).unwrap()).unwrap();
    assert_eq!(rows.len(), 721);
    assert_eq!(
        checker::payload(&checker::operations()).unwrap(),
        input,
        "complete unchanged input schedule"
    );
    checker::compare(
        &checker::source_trace(&checker::operations()).unwrap(),
        &rows,
    )
    .unwrap();
    let report = serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
    Original {
        directory,
        rows,
        input,
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
fn all_721_original_operations_and_rewrite_decisions_match_independent_source_and_real_native() {
    let original = original();
    let output = original.directory.path().join("native-checker");
    let report = checker::check(
        &options(&repository(), &candidate(), &output),
        &mut NativeRunner,
        &mut vec![],
        &AtomicBool::new(false),
    )
    .unwrap();
    for key in [
        "operations",
        "max_events_per_channel",
        "mode",
        "values_and_rewrite_decisions",
        "source_sha256",
        "binary_sha256",
    ] {
        assert_eq!(report[key], original.report[key], "{key}");
    }
    assert_eq!(report["operations"], 721);
    assert_eq!(report["max_events_per_channel"], 64);
    let evidence = PathBuf::from(
        std::env::var_os("BLUETOOTH_JOURNAL_EVIDENCE").expect("mandatory CI capture output"),
    );
    fs::create_dir_all(&evidence).unwrap();
    let normalized = checker::normalize_trace(&original.rows).unwrap();
    let text = normalized
        .rows
        .iter()
        .map(|r| r.to_json().unwrap() + "\n")
        .collect::<String>();
    fs::write(evidence.join("original-normalized.jsonl"), &text).unwrap();
    fs::write(evidence.join("original-observation.json"),serde_json::to_vec_pretty(&json!({"kind":"actual unchanged Python source trace, never native candidate observations","operations":721,"source_checker_sha256":sha(&fs::read(repository().join("rust/tools/check_bluetooth_journal.py")).unwrap()),"model_sha256":original.report["source_sha256"],"input_sha256":sha(&original.input),"normalized_trace_sha256":sha(text.as_bytes()),"normalized_trace_bytes":text.len(),"sessions":normalized.sessions.len(),"max_events":normalized.max_events})).unwrap()).unwrap();
    fs::write(
        evidence.join("native-report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
#[test]
fn actual_original_trace_mutations_preserve_every_field_and_original_acceptance_guard() {
    let original = original();
    let expected = checker::source_trace(&checker::operations()).unwrap();
    let original_lines = original
        .rows
        .iter()
        .map(|r| r.to_json().unwrap())
        .collect::<Vec<_>>();
    for mode in [
        "missing-row",
        "extra-row",
        "mode",
        "id",
        "second-session",
        "sequence",
        "action",
        "address",
        "hold",
        "repeat",
        "time",
        "missing-field",
        "extra-field",
        "rewrite",
        "extra-file",
        "over64",
    ] {
        // Mutations are based only on the actual original trace returned above.
        let mut lines = original_lines.clone();
        match mode {
            "missing-row" => {
                lines.pop();
            }
            "extra-row" => lines.push(lines.last().unwrap().clone()),
            "mode" => lines[1] = lines[1].replacen("\"mode\": 384", "\"mode\": 420", 1),
            "id" => {
                let session = checker::normalize_trace(&original.rows)
                    .unwrap()
                    .sessions
                    .into_iter()
                    .next()
                    .unwrap();
                lines[1] = lines[1].replace(&format!("{session}:1"), "BAD:1");
            }
            "second-session" => {
                let session = checker::normalize_trace(&original.rows)
                    .unwrap()
                    .sessions
                    .into_iter()
                    .next()
                    .unwrap();
                lines[1] = lines[1].replace(&session, "11111111111111111111111111111111");
            }
            "sequence" => lines[1] = lines[1].replace(":1\\\"", ":2\\\""),
            "action" => lines[1] = lines[1].replace("accelCruise", "cancel"),
            "address" => lines[1] = lines[1].replace("66:C0:0C:7B:6E:71", "00:00:00:00:00:00"),
            "hold" => {
                lines[304] = lines[304].replace("/dev/input/event0:up@long", "different-hold")
            }
            "repeat" => {
                lines[1] = lines[1].replace("\\\"repeat\\\": false", "\\\"repeat\\\": true")
            }
            "time" => lines[1] = lines[1].replace("10.0", "10.25"),
            "missing-field" | "extra-field" | "rewrite" | "extra-file" | "over64" => {
                // Outer structure contains no floats outside the bytes string.
                let mut row: Value = serde_json::from_str(&lines[1]).unwrap();
                if mode == "rewrite" {
                    let previous: Value = serde_json::from_str(&lines[0]).unwrap();
                    row[0]["inode"] = previous[0]["inode"].clone();
                } else if mode == "extra-file" {
                    let extra = row[0].clone();
                    row.as_array_mut().unwrap().push(extra);
                } else {
                    let mut journal: Value =
                        serde_json::from_str(row[0]["bytes"].as_str().unwrap()).unwrap();
                    if mode == "missing-field" {
                        journal["events"][0]
                            .as_object_mut()
                            .unwrap()
                            .remove("repeat");
                    } else if mode == "extra-field" {
                        journal["events"][0]["unobserved"] = json!(true);
                    } else {
                        let event = journal["events"][0].clone();
                        journal["events"] = json!(vec![event; 65]);
                    }
                    row[0]["bytes"] = json!(journal.to_string());
                }
                lines[1] = row.to_string();
            }
            _ => unreachable!(),
        }
        assert_ne!(lines, original_lines, "ineffective source mutation: {mode}");
        let rows = lines
            .iter()
            .map(|s| JsonValue::parse(s).unwrap())
            .collect::<Vec<_>>();
        assert!(
            checker::compare(&expected, &rows).is_err(),
            "accepted {mode}"
        );
    }
}
fn source_copy(root: &Path) {
    let pins: Value =
        serde_json::from_str(include_str!("fixtures/bluetooth-journal/source-pins.json")).unwrap();
    for name in pins.as_object().unwrap().keys() {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(repository().join(name), path).unwrap();
    }
}
struct Peer<'a> {
    original: &'a Original,
    mode: &'a str,
    source: PathBuf,
    alias: PathBuf,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::from_secs(30));
        assert_eq!(spec.input.as_ref().unwrap(), &self.original.input);
        let argv = spec.effective_argv();
        assert_eq!(argv.len(), 2);
        assert!(Path::new(&argv[0]).is_absolute());
        let root = Path::new(&argv[1]);
        fs::create_dir_all(root)?;
        for name in ["cruise.json", "lane.json"] {
            fs::copy(
                self.original
                    .directory
                    .path()
                    .join("source-run/original")
                    .join(name),
                root.join(name),
            )?;
        }
        match self.mode {
            "temporary" => fs::write(root.join("leftover.tmp"), b"unremoved")?,
            "source" => fs::write(&self.source, b"changed")?,
            "artifact" => fs::write(&self.alias, b"changed artifact")?,
            "retarget" => {
                let other = self.alias.with_extension("other");
                fs::write(&other, b"same candidate")?;
                fs::remove_file(&self.alias)?;
                symlink(other, &self.alias)?;
            }
            "timeout" => return Err("injected timeout".into()),
            _ => {}
        }
        let text = self
            .original
            .rows
            .iter()
            .map(|r| r.to_json().unwrap() + "\n")
            .collect::<String>();
        Ok(Captured {
            code: if self.mode == "exit" { 9 } else { 0 },
            stdout: if self.mode == "decode" {
                vec![255]
            } else {
                text.into_bytes()
            },
            stderr: b"diagnostic retained\n".to_vec(),
        })
    }
}
#[test]
fn original_sourced_process_identity_and_receipt_failures_cannot_publish_success() {
    let original = original();
    for mode in [
        "ok",
        "temporary",
        "source",
        "artifact",
        "retarget",
        "timeout",
        "exit",
        "decode",
        "foreign",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        source_copy(&root);
        let target = dir.path().join("target");
        fs::write(&target, b"same candidate").unwrap();
        let alias = dir.path().join("candidate");
        symlink(&target, &alias).unwrap();
        let output = dir.path().join("output");
        if mode == "foreign" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("result.json"), b"foreign receipt").unwrap();
        }
        let mut peer = Peer {
            original: &original,
            mode,
            source: root.join("openpilot/selfdrive/carrot/bluetooth/model.py"),
            alias: alias.clone(),
            calls: 0,
        };
        let observed = checker::check(
            &options(&root, &alias, &output),
            &mut peer,
            &mut vec![],
            &AtomicBool::new(false),
        );
        assert_eq!(observed.is_ok(), mode == "ok", "{mode}: {observed:?}");
        assert_eq!(peer.calls, 1);
        if mode == "foreign" {
            assert_eq!(
                fs::read(output.join("result.json")).unwrap(),
                b"foreign receipt"
            );
        } else {
            assert_eq!(output.join("result.json").exists(), observed.is_ok());
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
    struct Cancel<'a>(&'a AtomicBool);
    impl Write for Cancel<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("binary");
    fs::write(&binary, b"owned identity").unwrap();
    let mut peer = Peer {
        original: &original,
        mode: "ok",
        source: PathBuf::new(),
        alias: binary.clone(),
        calls: 0,
    };
    let output = dir.path().join("broken");
    assert!(checker::check(
        &options(&repository(), &binary, &output),
        &mut peer,
        &mut Broken,
        &AtomicBool::new(false)
    )
    .is_err());
    assert!(!output.join("result.json").exists());
    let output = dir.path().join("cancel");
    let stop = AtomicBool::new(false);
    assert!(checker::check(
        &options(&repository(), &binary, &output),
        &mut peer,
        &mut Cancel(&stop),
        &stop
    )
    .is_err());
    assert!(!output.join("result.json").exists());
}
#[cfg(target_os = "linux")]
#[test]
fn missing_changed_dangling_fifo_inputs_reject_without_execution_and_preserve_foreign_report() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("execution before input admission")
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
            fs::write(output.join("result.json"), b"foreign receipt").unwrap();
            assert!(checker::check(
                &options(&root, &binary, &output),
                &mut Never,
                &mut vec![],
                &AtomicBool::new(false)
            )
            .is_err());
            assert_eq!(
                fs::read(output.join("result.json")).unwrap(),
                b"foreign receipt"
            );
        }
    }
}
#[cfg(target_os = "linux")]
#[test]
fn actual_cli_argument_admission_and_raw_paths() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let checker = env!("CARGO_BIN_EXE_check-bluetooth-journal");
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
        [("OPENPILOT_RUST_ROOT".into(), repository().into_os_string())]
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
    assert_eq!(report["operations"], OPERATIONS);
    assert_eq!(report["max_events_per_channel"], 64);
    assert_eq!(
        report,
        serde_json::from_slice::<Value>(&fs::read(output.join("result.json")).unwrap()).unwrap()
    );
}
#[cfg(target_os = "linux")]
#[path = "bluetooth_journal_check/lifecycle.rs"]
mod lifecycle;
