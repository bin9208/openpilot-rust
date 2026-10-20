#![cfg(feature = "bluetooth-config-native")]
use openpilot_dev_tools::{
    bluetooth_config_check::{self as checker, Options},
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn candidate() -> PathBuf {
    fs::canonicalize(
        std::env::var_os("BLUETOOTH_CONFIG_FIXTURE").expect("mandatory fresh config_fixture"),
    )
    .unwrap()
}
fn original(output: &Path) -> Value {
    let mut command = CommandSpec::new_os(
        vec![
            std::env::var_os("BLUETOOTH_CONFIG_PYTHON")
                .expect("mandatory Python 3.12 original oracle"),
            root()
                .join("rust/tools/check_bluetooth_config.py")
                .into_os_string(),
            "--binary".into(),
            candidate().into_os_string(),
            "--output".into(),
            output.as_os_str().into(),
        ],
        Duration::from_secs(60),
    );
    command.cwd = Some(root());
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    let result = NativeRunner.run(&command).unwrap();
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap()
}
fn rows(path: &Path) -> Vec<JsonValue> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| JsonValue::parse(line).unwrap())
        .collect()
}
fn order(value: &JsonValue) -> Vec<Vec<u32>> {
    match value.view() {
        JsonView::Object(items) => items.into_iter().map(|(key, _)| key.to_vec()).collect(),
        _ => panic!("expected ordered object"),
    }
}
#[test]
fn unchanged_original_dynamic_full_schedule_response_order_and_report_match_native() {
    let dir = tempfile::tempdir().unwrap();
    let original_dir = dir.path().join("python-original");
    let report = original(&original_dir);
    let native_dir = dir.path().join("rust-native");
    let native = checker::check(
        &Options {
            root: root(),
            binary: candidate(),
            output: native_dir.clone(),
        },
        &mut NativeRunner,
        &mut vec![],
        &AtomicBool::new(false),
    )
    .unwrap();
    let source_input = rows(&original_dir.join("input.jsonl"));
    let native_input = checker::cases().unwrap();
    assert_eq!(source_input.len(), 253, "complete pinned original roster");
    assert_eq!(source_input.len(), native_input.len());
    assert_eq!(
        fs::read(original_dir.join("input.jsonl")).unwrap(),
        fs::read(native_dir.join("input.jsonl")).unwrap(),
        "full input JSONL bytes"
    );
    for (index, (source, native)) in source_input.iter().zip(&native_input).enumerate() {
        assert_eq!(
            source.to_json().unwrap(),
            native.to_json().unwrap(),
            "input {index}"
        );
    }
    let source = rows(&original_dir.join("original.jsonl"));
    let expected = rows(&native_dir.join("original.jsonl"));
    let actual = rows(&native_dir.join("native.jsonl"));
    assert_eq!(source.len(), expected.len());
    assert_eq!(source.len(), actual.len());
    for (index, ((source, expected), actual)) in
        source.iter().zip(&expected).zip(&actual).enumerate()
    {
        assert_eq!(
            source.to_json().unwrap(),
            expected.to_json().unwrap(),
            "original response {index}"
        );
        assert_eq!(
            source.to_json().unwrap(),
            actual.to_json().unwrap(),
            "native response {index}"
        );
        if let Some(devices) = source.get("devices") {
            for other in [expected, actual] {
                let other_devices = other.get("devices").unwrap();
                assert_eq!(order(&devices), order(&other_devices), "devices {index}");
                let JsonView::Object(left) = devices.view() else {
                    panic!()
                };
                let JsonView::Object(right) = other_devices.view() else {
                    panic!()
                };
                for ((_, a), (_, b)) in left.into_iter().zip(right) {
                    assert_eq!(
                        order(&a.get("mapping").unwrap()),
                        order(&b.get("mapping").unwrap()),
                        "mapping {index}"
                    );
                }
            }
        }
    }
    for key in ["cases", "accepted", "source_sha256", "binary_sha256"] {
        assert_eq!(report[key], native[key], "summary {key}");
    }
    let evidence = PathBuf::from(
        std::env::var_os("BLUETOOTH_CONFIG_EVIDENCE").expect("mandatory CI evidence output"),
    );
    fs::create_dir_all(&evidence).unwrap();
    for name in ["input.jsonl", "original.jsonl", "result.json"] {
        fs::copy(
            original_dir.join(name),
            evidence.join(format!("python-{name}")),
        )
        .unwrap();
    }
    fs::copy(
        native_dir.join("result.json"),
        evidence.join("native-result.json"),
    )
    .unwrap();
}

struct Peer {
    input: Vec<u8>,
    response: Vec<u8>,
    mode: &'static str,
    calls: usize,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::from_secs(30));
        assert_eq!(spec.input.as_ref().unwrap(), &self.input);
        assert_eq!(spec.effective_argv().len(), 1);
        if self.mode == "timeout" {
            return Err("injected deadline".into());
        }
        let mut response = self.response.clone();
        match self.mode {
            "missing" => {
                response.truncate(response.iter().rposition(|v| *v == b'\n').unwrap());
                response.truncate(response.iter().rposition(|v| *v == b'\n').unwrap() + 1);
            }
            "extra" => response.extend_from_slice(b"{}\n"),
            "invalid" => response = b"not JSON\n".to_vec(),
            "mismatch" => {
                response = [
                    b"{\"error\": \"different\"}\n".as_slice(),
                    &response[response.iter().position(|v| *v == b'\n').unwrap() + 1..],
                ]
                .concat()
            }
            "reorder" => {
                let text = String::from_utf8(response).unwrap();
                let changed = text.replacen(
                    "\"key:0\": \"none\", \"key:1\": \"none\"",
                    "\"key:1\": \"none\", \"key:0\": \"none\"",
                    1,
                );
                assert_ne!(text, changed, "original mapping order control missing");
                response = changed.into_bytes();
            }
            _ => {}
        }
        Ok(Captured {
            code: if self.mode == "exit" { 9 } else { 0 },
            stdout: response,
            stderr: b"diagnostic\n".to_vec(),
        })
    }
}
#[test]
fn original_sourced_failure_boundaries_keep_receipt_unpublished() {
    let dir = tempfile::tempdir().unwrap();
    let source_dir = dir.path().join("original");
    original(&source_dir);
    let input = fs::read(source_dir.join("input.jsonl")).unwrap();
    let response = fs::read(source_dir.join("original.jsonl")).unwrap();
    for mode in [
        "ok", "missing", "extra", "invalid", "mismatch", "reorder", "exit", "timeout", "foreign",
    ] {
        let output = dir.path().join(mode);
        if mode == "foreign" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("result.json"), b"foreign").unwrap();
        }
        let mut peer = Peer {
            input: input.clone(),
            response: response.clone(),
            mode,
            calls: 0,
        };
        let result = checker::check(
            &Options {
                root: root(),
                binary: candidate(),
                output: output.clone(),
            },
            &mut peer,
            &mut vec![],
            &AtomicBool::new(false),
        );
        assert_eq!(result.is_ok(), mode == "ok", "{mode}: {result:?}");
        assert_eq!(peer.calls, usize::from(mode != "foreign"));
        if mode == "foreign" {
            assert_eq!(fs::read(output.join("result.json")).unwrap(), b"foreign");
        } else {
            assert_eq!(output.join("result.json").exists(), mode == "ok", "{mode}");
        }
    }
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut peer = Peer {
        input: input.clone(),
        response: response.clone(),
        mode: "ok",
        calls: 0,
    };
    let output = dir.path().join("broken");
    assert!(checker::check(
        &Options {
            root: root(),
            binary: candidate(),
            output: output.clone()
        },
        &mut peer,
        &mut Broken,
        &AtomicBool::new(false)
    )
    .is_err());
    assert!(!output.join("result.json").exists());

    struct CancelOnFlush<'a>(&'a AtomicBool);
    impl Write for CancelOnFlush<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }
    let stop = AtomicBool::new(false);
    let output = dir.path().join("cancel-at-publication");
    let mut peer = Peer {
        input,
        response,
        mode: "ok",
        calls: 0,
    };
    assert!(checker::check(
        &Options {
            root: root(),
            binary: candidate(),
            output: output.clone()
        },
        &mut peer,
        &mut CancelOnFlush(&stop),
        &stop,
    )
    .is_err());
    assert!(!output.join("result.json").exists());
}

#[test]
fn required_sources_and_cancellation_reject_before_candidate_invocation() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("candidate invoked before admission")
        }
    }
    let dir = tempfile::tempdir().unwrap();
    for source in [
        "rust/tools/check_bluetooth_config.py",
        "openpilot/selfdrive/carrot/bluetooth/model.py",
        "rust/crates/bluetooth/examples/config_fixture.rs",
    ] {
        for kind in ["missing", "changed"] {
            if source == "rust/tools/check_bluetooth_config.py" && kind == "missing" {
                continue; // The retired checker is the sole optional historical source.
            }
            let private = dir
                .path()
                .join(format!("{}-{kind}", source.replace('/', "-")));
            for name in [
                "rust/tools/check_bluetooth_config.py",
                "openpilot/selfdrive/carrot/bluetooth/model.py",
                "rust/crates/bluetooth/examples/config_fixture.rs",
            ] {
                let target = private.join(name);
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                fs::copy(root().join(name), &target).unwrap();
            }
            let target = private.join(source);
            fs::remove_file(&target).unwrap();
            if kind == "changed" {
                fs::write(&target, b"changed pinned source").unwrap();
            }
            let output = dir
                .path()
                .join(format!("output-{}-{kind}", source.replace('/', "-")));
            assert!(checker::check(
                &Options {
                    root: private,
                    binary: candidate(),
                    output: output.clone()
                },
                &mut Never,
                &mut vec![],
                &AtomicBool::new(false)
            )
            .is_err());
            assert!(!output.exists());
        }
    }
    let output = dir.path().join("cancelled");
    assert!(checker::check(
        &Options {
            root: root(),
            binary: candidate(),
            output: output.clone()
        },
        &mut Never,
        &mut vec![],
        &AtomicBool::new(true)
    )
    .is_err());
    assert!(!output.exists());
}

#[test]
fn absent_historical_checker_is_held_through_publication_and_reappearance_fails() {
    let dir = tempfile::tempdir().unwrap();
    let private = dir.path().join("source");
    fs::create_dir_all(private.join("rust/tools")).unwrap();
    for name in [
        "openpilot/selfdrive/carrot/bluetooth/model.py",
        "rust/crates/bluetooth/examples/config_fixture.rs",
    ] {
        let target = private.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(name), target).unwrap();
    }
    let input =
        include_bytes!("fixtures/bluetooth-config/original-observation-39507901490/input.jsonl");
    let response =
        include_bytes!("fixtures/bluetooth-config/original-observation-39507901490/original.jsonl");
    let peer = || Peer {
        input: input.to_vec(),
        response: response.to_vec(),
        mode: "ok",
        calls: 0,
    };
    let options = |name| Options {
        root: private.clone(),
        binary: candidate(),
        output: dir.path().join(name),
    };
    let mut actual = peer();
    let output = options("absent-accepted");
    checker::check(&output, &mut actual, &mut vec![], &AtomicBool::new(false)).unwrap();
    assert_eq!(actual.calls, 1);
    assert!(output.output.join("result.json").is_file());

    struct Reappear(PathBuf);
    impl Write for Reappear {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            fs::write(&self.0, b"reappeared")
        }
    }
    let historical = private.join("rust/tools/check_bluetooth_config.py");
    let mut actual = peer();
    let output = options("reappeared-at-publication");
    assert!(checker::check(
        &output,
        &mut actual,
        &mut Reappear(historical.clone()),
        &AtomicBool::new(false),
    )
    .is_err());
    assert_eq!(actual.calls, 1);
    assert!(!output.output.join("result.json").exists());

    fs::remove_file(&historical).unwrap();
    std::os::unix::fs::symlink(
        root().join("rust/tools/check_bluetooth_config.py"),
        &historical,
    )
    .unwrap();
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("candidate invoked before historical symlink rejection")
        }
    }
    let output = options("historical-symlink");
    assert!(checker::check(&output, &mut Never, &mut vec![], &AtomicBool::new(false)).is_err());
    assert!(!output.output.exists());
}

#[test]
fn cli_argument_admission_and_real_candidate_receipt() {
    let executable = env!("CARGO_BIN_EXE_check-bluetooth-config");
    for args in [
        vec!["--help"],
        vec![],
        vec!["--unknown"],
        vec!["--binary", "value"],
    ] {
        let command = CommandSpec::new(
            args.iter()
                .fold(vec![executable.into()], |mut values, value| {
                    values.push((*value).into());
                    values
                }),
            Duration::from_secs(5),
        );
        let result = NativeRunner.run(&command).unwrap();
        assert_eq!(result.code, if args == ["--help"] { 0 } else { 2 });
    }
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("actual-cli");
    let mut command = CommandSpec::new_os(
        vec![
            executable.into(),
            "--binary".into(),
            candidate().into_os_string(),
            "--output".into(),
            output.as_os_str().into(),
        ],
        Duration::from_secs(45),
    );
    command.os_env = Some(
        [(
            std::ffi::OsString::from("OPENPILOT_RUST_ROOT"),
            root().into_os_string(),
        )]
        .into_iter()
        .collect(),
    );
    let result = NativeRunner.run(&command).unwrap();
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout: Value = serde_json::from_slice(&result.stdout).unwrap();
    let receipt: Value =
        serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
    assert_eq!(stdout, receipt);
    assert_eq!(receipt["cases"], checker::cases().unwrap().len());
}

#[cfg(target_os = "linux")]
#[path = "bluetooth_config_check/lifecycle.rs"]
mod lifecycle;
