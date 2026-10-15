use openpilot_dev_tools::{
    logger_identifier_check::{self, Case},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/logger-identifier/original.json")).unwrap()
}
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks(2)
        .map(|s| u8::from_str_radix(std::str::from_utf8(s).unwrap(), 16).unwrap())
        .collect()
}
fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}
fn normalize(value: &Value, root: &Path) -> Value {
    match value {
        Value::String(s) => json!(s.replace(root.to_str().unwrap(), "$ROOT")),
        Value::Array(values) => Value::Array(values.iter().map(|v| normalize(v, root)).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(k, v)| (k.clone(), normalize(v, root)))
                .collect(),
        ),
        v => v.clone(),
    }
}
struct Peer {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    code: i32,
    calls: Vec<Value>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let params = PathBuf::from(&spec.env["PARAMS_ROOT"]).join("d");
        let counter = params.join(spec.argv.last().unwrap());
        let is_file = counter.is_file();
        let input = if is_file && mode(&counter) & 0o400 != 0 {
            Some(hex(&fs::read(&counter)?))
        } else {
            None
        };
        self.calls.push(json!({"argv":spec.argv,"environment":spec.env,"params_mode":mode(&params),"counter_mode":mode(&counter),"counter_kind":if is_file{"file"}else{"directory"},"input_hex":input}));
        Ok(Captured {
            code: self.code,
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
        })
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn peer(oracle: &Value) -> Peer {
    Peer {
        stdout: oracle["success_stdout"]
            .as_str()
            .unwrap()
            .as_bytes()
            .to_vec(),
        stderr: oracle["success_stderr"]
            .as_str()
            .unwrap()
            .as_bytes()
            .to_vec(),
        code: 0,
        calls: vec![],
    }
}
#[test]
fn original_eighty_cases_keep_inputs_environment_modes_results_and_artifacts() {
    let original = oracle();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let binary = root.join("binary");
    fs::write(&binary, original["binary_contents"].as_str().unwrap()).unwrap();
    let control_dir = root.join("control");
    fs::create_dir(&control_dir).unwrap();
    let control_file = root.join("control-file");
    fs::write(&control_file, b"").unwrap();
    let dir_mode = mode(&control_dir);
    let file_mode = mode(&control_file);
    for captured in original["cases"].as_array().unwrap() {
        let key = captured["key"].as_str().unwrap();
        let failure = captured["failure"].as_str().unwrap();
        let directory = root.join(format!("{key}-{}-{failure}", captured["index"]));
        let input = bytes(captured["value_hex"].as_str().unwrap());
        let case = Case {
            key: key.into(),
            value: input,
            failure: failure.into(),
        };
        let mut runner = peer(&original);
        let result = logger_identifier_check::run_case(
            &binary,
            &case,
            &directory,
            &["fixture-runner".into(), "runner-arg".into()],
            &mut runner,
        )
        .unwrap();
        assert_eq!(result, captured["result"], "{directory:?}");
        assert_eq!(runner.calls.len(), 1);
        let mut expected = captured["call"].clone();
        if failure != "unwritable" {
            expected["params_mode"] = json!(dir_mode);
        }
        if failure != "unreadable" {
            expected["counter_mode"] = json!(if failure == "directory" {
                dir_mode
            } else {
                file_mode
            });
        }
        assert_eq!(normalize(&runner.calls[0], root), expected, "{directory:?}");
        for (name, want) in captured["files"].as_object().unwrap() {
            let content = fs::read(directory.join(name)).unwrap();
            let actual = if name.ends_with(".json") {
                serde_json::from_slice(&content).unwrap()
            } else {
                json!(String::from_utf8(content).unwrap())
            };
            assert_eq!(normalize(&actual, root), *want, "{directory:?} {name}");
        }
        assert_eq!(mode(&directory.join("params/d")), 0o700);
        assert_eq!(
            mode(&directory.join("params/d").join(key)),
            if failure == "directory" {
                dir_mode
            } else {
                0o600
            }
        );
    }
}
#[test]
fn original_failure_artifacts_and_permission_restoration_precede_rejection() {
    let original = oracle();
    for failure in original["failures"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("binary");
        fs::write(&binary, original["binary_contents"].as_str().unwrap()).unwrap();
        let directory = temp.path().join("case");
        let mut runner = Peer {
            stdout: bytes(failure["stdout_hex"].as_str().unwrap()),
            stderr: bytes(failure["stderr_hex"].as_str().unwrap()),
            code: failure["code"].as_i64().unwrap() as i32,
            calls: vec![],
        };
        let case = Case {
            key: "RouteCount".into(),
            value: b"0".to_vec(),
            failure: "unwritable".into(),
        };
        assert!(
            logger_identifier_check::run_case(&binary, &case, &directory, &[], &mut runner)
                .is_err()
        );
        let mut files = fs::read_dir(&directory)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(json!(files), failure["files"]);
        assert_eq!(
            fs::read(directory.join("stdout.log")).unwrap(),
            bytes(failure["stdout_log_hex"].as_str().unwrap())
        );
        assert_eq!(
            fs::read(directory.join("stderr.log")).unwrap(),
            bytes(failure["stderr_log_hex"].as_str().unwrap())
        );
        assert_eq!(mode(&directory.join("params/d")), 0o700);
        assert_eq!(mode(&directory.join("params/d/RouteCount")), 0o600);
    }
}
#[test]
fn case_inventory_is_the_original_two_keys_ten_byte_inputs_and_four_failures() {
    let original = oracle();
    let cases = logger_identifier_check::cases();
    assert_eq!(cases.len(), 80);
    for (actual, want) in cases.iter().zip(original["cases"].as_array().unwrap()) {
        assert_eq!(actual.key, want["key"]);
        assert_eq!(hex(&actual.value), want["value_hex"]);
        assert_eq!(actual.failure, want["failure"]);
    }
}
#[test]
fn actual_cli_rejects_missing_positional_arguments_and_unknown_options() {
    for args in [
        vec![],
        vec!["--unknown"],
        vec!["binary", "original", "output", "extra"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_check-logger-identifier"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
#[test]
fn mismatched_source_and_native_results_never_publish_overall_success() {
    struct Mismatch(usize);
    impl Runner for Mismatch {
        fn run(&mut self, _spec: &CommandSpec) -> Result<Captured> {
            self.0 += 1;
            Ok(Captured {
                code: 0,
                stdout: format!(
                    "{}--0123456789\n",
                    if self.0 == 1 { "0000002a" } else { "0000002b" }
                )
                .into_bytes(),
                stderr: vec![],
            })
        }
    }
    let original = oracle();
    let captured = &original["comparison_failure"];
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("binary");
    fs::write(&binary, original["binary_contents"].as_str().unwrap()).unwrap();
    let output = temp.path().join("output");
    let mut peer = Mismatch(0);
    let args = vec![
        binary.display().to_string(),
        binary.display().to_string(),
        output.display().to_string(),
    ];
    let error = logger_identifier_check::run(&args, &mut peer).unwrap_err();
    assert!(error.to_string().contains("identifier mismatch"));
    assert_eq!(peer.0, captured["calls"].as_u64().unwrap() as usize);
    assert!(!output.join("result.json").exists());
    for kind in ["source", "native"] {
        let actual: Value = serde_json::from_slice(
            &fs::read(
                output
                    .join("RouteCount-0-ordinary")
                    .join(kind)
                    .join("result.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(actual, captured[kind]);
    }
}
#[test]
fn runner_flag_values_are_parse_errors_before_any_output_or_source_execution() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("executed");
    let binary = temp.path().join("binary");
    fs::write(
        &binary,
        format!(
            "#!/bin/sh\ntouch '{}'\nprintf '0000002a--0123456789\\n'\n",
            marker.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    for value in ["--unknown", "--help"] {
        let output = temp.path().join(value.trim_start_matches('-'));
        let result = Command::new(env!("CARGO_BIN_EXE_check-logger-identifier"))
            .arg(&binary)
            .arg(&binary)
            .arg(&output)
            .args(["--runner", value])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2), "{result:?}");
        assert!(!output.exists());
        assert!(!marker.exists());
    }
}
#[test]
fn actual_raw_paths_and_ordered_append_runners_preserve_invocation_and_environment_bytes() {
    use std::{
        ffi::OsString,
        os::unix::ffi::{OsStrExt, OsStringExt},
    };
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join(OsString::from_vec(b"peer-\xff".to_vec()));
    let output = temp.path().join(OsString::from_vec(b"out-\xff".to_vec()));
    let marker = temp.path().join("env");
    fs::write(&binary,b"#!/bin/sh\nprintf '%s\\0' \"$PARAMS_ROOT\" >> \"$ENV_BYTES\"\nprintf '0000002a--0123456789\\n'\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let prefix = temp
        .path()
        .join(OsString::from_vec(b"runner-\xff".to_vec()));
    fs::write(
        &prefix,
        b"#!/bin/sh\n[ \"$1\" = marker ] || exit 9\nshift\nexec \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(&prefix, fs::Permissions::from_mode(0o755)).unwrap();
    let mut inline = OsString::from("--r=");
    inline.push(&prefix);
    let result = Command::new(env!("CARGO_BIN_EXE_check-logger-identifier"))
        .arg(&binary)
        .arg(&binary)
        .arg(&output)
        .arg(inline)
        .arg("--runner=marker")
        .env("ENV_BYTES", &marker)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let env = fs::read(marker).unwrap();
    let entries = env
        .split(|&b| b == 0)
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 160);
    assert!(entries.iter().all(|v| v.contains(&255)));
    assert!(entries
        .iter()
        .all(|v| v.starts_with(output.as_os_str().as_bytes())));
    let invocation =
        fs::read_to_string(output.join("RouteCount-0-ordinary/native/invocation.json")).unwrap();
    assert!(invocation.contains("runner-\\udcff"));
    assert!(invocation.contains("peer-\\udcff"));
    assert!(!invocation.contains('\u{fffd}'));
    let json = openpilot_log_format::JsonValue::parse(&invocation).unwrap();
    let openpilot_log_format::JsonView::Array(argv) = json.get("argv").unwrap().view() else {
        panic!("argv")
    };
    assert_eq!(argv.len(), 4);
    assert!(argv[1].text_eq("marker"));
}
