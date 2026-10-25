//! Original packet generation, child ordering and exact artifact expectations.
use openpilot_dev_tools::{
    camera_packet_check::{cases, check, inputs},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fixture(name: &str) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/camera-packets-check")
                .join(name),
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn every_original_case_and_all_66_batch_inputs_are_exact() {
    let expected = fixture("generation-original.json");
    let cases = cases().unwrap();
    assert_eq!(cases.len(), 3281);
    for (case, hash) in cases
        .iter()
        .zip(expected["case_sha256"].as_array().unwrap())
    {
        assert_eq!(
            sha(case.to_json().unwrap().as_bytes()),
            hash.as_str().unwrap()
        );
    }
    let full = cases
        .iter()
        .map(|v| v.to_json().unwrap() + "\n")
        .collect::<String>();
    assert_eq!(sha(full.as_bytes()), expected["inputs_jsonl_sha256"]);
    for (batch, expected) in cases
        .chunks(50)
        .zip(expected["batches"].as_array().unwrap())
    {
        let (source, native) = inputs(batch).unwrap();
        assert_eq!(sha(source.as_bytes()), expected["source_sha256"]);
        assert_eq!(sha(native.as_bytes()), expected["native_sha256"]);
        assert_eq!(batch.len(), expected["cases"].as_u64().unwrap() as usize);
    }
}
struct Peer<'a> {
    case: &'a serde_json::Value,
    root: &'a Path,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let expected = &self.case["calls"][self.calls];
        let first = self.calls == 0;
        let lane = if first { "source" } else { "native" };
        let mode = self.case["name"].as_str().unwrap();
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::from_secs(120));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(!spec.merge_output && !spec.inherit_output && !spec.inherit_stdin);
        assert_eq!(
            spec.input.as_deref(),
            Some(expected["input"].as_str().unwrap().as_bytes())
        );
        let argv = spec
            .effective_argv()
            .iter()
            .map(|v| {
                v.to_string_lossy()
                    .replace(self.root.to_str().unwrap(), "$ROOT")
            })
            .collect::<Vec<_>>();
        assert_eq!(serde_json::json!(argv), expected["argv"]);
        if mode == format!("{lane}-spawn") || mode == format!("{lane}-timeout") {
            return Err("owned process failure".into());
        }
        if mode == format!("{lane}-utf8") {
            return Ok(Captured {
                code: 0,
                stdout: vec![255],
                stderr: vec![],
            });
        }
        if mode == format!("{lane}-stderr-utf8") {
            return Ok(Captured {
                code: 0,
                stdout: b"{}\n[]\n[]\n".to_vec(),
                stderr: vec![255],
            });
        }
        if !first && mode.starts_with("hash-") {
            fs::remove_file(self.root.join(if mode == "hash-source-missing" {
                "reference"
            } else {
                "native"
            }))?;
        }
        let mut values = vec!["{}", "[[1, 2], [3]]", "[[4, 5]]"];
        if ["mismatch", "failure-directory"].contains(&mode) {
            values[1] = if first {
                "[[1, 2], [3]]"
            } else {
                "[[1, 9], [3]]"
            };
        }
        if mode == "key-mismatch" {
            values[0] = if first { r#"{"x":1}"# } else { r#"{"y":1}"# };
        }
        if mode == format!("{lane}-count") {
            values.pop();
        }
        if mode == format!("{lane}-json") {
            values[1] = "invalid";
        }
        if mode == "length-scalar" {
            values[1] = "[1]";
        }
        if mode == "length-text" {
            values[1] = r#""é😀""#;
        }
        if mode == "length-object" {
            values[1] = r#"{"é😀":1,"a":2}"#;
        }
        if mode == "surrogate" {
            values[1] = if first { r#""\ud800""# } else { r#""x""# };
        }
        let sep = if mode == "splitlines" { "\r\n" } else { "\n" };
        let stdout = (values.join(sep) + sep).into_bytes();
        Ok(Captured {
            code: if mode == format!("{lane}-exit") { 7 } else { 0 },
            stdout,
            stderr: format!("{lane} stderr\n").into_bytes(),
        })
    }
}
#[test]
fn original_packet_failure_stages_byte_counts_and_no_newline_failure_receipt() {
    let record = fixture("process-original.json");
    let inputs = record["inputs_json"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| JsonValue::parse(v.as_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    for case in record["cases"].as_array().unwrap() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let reference = root.join("reference");
        let native = root.join("native");
        let output = root.join("out");
        fs::write(&reference, b"reference bytes").unwrap();
        fs::write(&native, b"native bytes").unwrap();
        let mode = case["name"].as_str().unwrap();
        if mode == "output-file" {
            fs::write(&output, "old output\n").unwrap();
        }
        if mode == "old-failure" || mode.ends_with("-directory") {
            fs::create_dir(&output).unwrap();
            if mode == "old-failure" {
                fs::write(output.join("failure.json"), "old failure\n").unwrap();
            } else {
                let name = match mode {
                    "inputs-directory" => "inputs.jsonl",
                    "source-stdout-directory" => "batch-000.source.jsonl",
                    "source-stderr-directory" => "batch-000.source.stderr",
                    "native-stdout-directory" => "batch-000.native.jsonl",
                    "native-stderr-directory" => "batch-000.native.stderr",
                    "failure-directory" => "failure.json",
                    "report-directory" => "report.json",
                    _ => panic!(),
                };
                fs::create_dir(output.join(name)).unwrap();
            }
        }
        let sr = match mode {
            "bad-source-runner" => "'broken",
            "runner-words" => "one \"two three\" \"\"",
            _ => "",
        };
        let nr = match mode {
            "bad-native-runner" => "\"broken",
            "runner-words" => "four 'five six'",
            _ => "",
        };
        let mut peer = Peer {
            case,
            root,
            calls: 0,
        };
        let mut printed = vec![];
        let result = check(
            &reference,
            &native,
            &output,
            sr,
            nr,
            &inputs,
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{mode}: {result:?}"
        );
        assert_eq!(
            peer.calls,
            case["calls"].as_array().unwrap().len(),
            "{mode}"
        );
        assert_eq!(
            String::from_utf8(printed)
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
            case["stdout"].as_str().unwrap(),
            "{mode}"
        );
        assert_eq!(
            if output.is_dir() {
                "directory"
            } else if output.exists() {
                "file"
            } else {
                "missing"
            },
            case["output_kind"],
            "{mode}"
        );
        let mut observed = serde_json::Map::new();
        if output.is_dir() {
            for entry in fs::read_dir(output).unwrap() {
                let path = entry.unwrap().path();
                observed.insert(
                    path.file_name().unwrap().to_str().unwrap().into(),
                    serde_json::Value::String(if path.is_dir() {
                        "directory".into()
                    } else {
                        fs::read_to_string(path)
                            .unwrap()
                            .replace(root.to_str().unwrap(), "$ROOT")
                    }),
                );
            }
        }
        assert_eq!(serde_json::Value::Object(observed), case["files"], "{mode}");
    }
}
#[test]
fn original_cli_grammar_and_missing_input_classes() {
    for case in fixture("cli-original.json")["cases"].as_array().unwrap() {
        let tmp = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-packets"))
            .args(
                case["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .current_dir(tmp.path())
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{case}"
        );
        assert_eq!(!result.stdout.is_empty(), case["stdout"]);
        assert_eq!(!result.stderr.is_empty(), case["stderr"]);
    }
}
#[cfg(unix)]
#[test]
fn actual_native_cli_preserves_raw_runner_output_paths_and_original_lexical_commands() {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    };
    let expected = fixture("raw-cli-original.json");
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let runner = root.join(OsString::from_vec(b"runner-\xff".to_vec()));
    let output = OsString::from_vec(b"output-\xff".to_vec());
    fs::write(
        &runner,
        "#!/bin/sh\nwhile IFS= read -r line; do printf '[]\\n'; done\n",
    )
    .unwrap();
    fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.join("source"), b"source").unwrap();
    fs::write(root.join("native"), b"native").unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-packets"))
        .current_dir(root)
        .env("PATH", "/nonexistent")
        .args(["--reference", "./source", "--binary", "native", "--output"])
        .arg(&output)
        .arg("--reference-runner")
        .arg(&runner)
        .arg("--runner")
        .arg(&runner)
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(expected["exit"].as_i64().unwrap() as i32),
        "{:?}",
        result.stderr
    );
    assert_eq!(
        String::from_utf8(result.stdout)
            .unwrap()
            .replace(root.to_str().unwrap(), "$ROOT"),
        expected["stdout"]
    );
    assert_eq!(
        String::from_utf8(result.stderr)
            .unwrap()
            .replace(root.to_str().unwrap(), "$ROOT"),
        expected["stderr"]
    );
    assert_eq!(
        fs::read_to_string(root.join(&output).join("report.json"))
            .unwrap()
            .replace(root.to_str().unwrap(), "$ROOT"),
        expected["report_json"]
    );
    assert_eq!(
        fs::read_dir(root.join(output)).unwrap().count(),
        expected["files"].as_u64().unwrap() as usize
    );
}
#[test]
fn original_parser_and_failure_receipt_depth_boundaries_are_separate() {
    struct Deep {
        depth: usize,
        mismatch: bool,
        calls: usize,
    }
    impl Runner for Deep {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            let leaf = if self.calls == 1 && self.mismatch {
                "2"
            } else {
                "1"
            };
            self.calls += 1;
            Ok(Captured {
                code: 0,
                stdout: ("[".repeat(self.depth) + leaf + &"]".repeat(self.depth) + "\n")
                    .into_bytes(),
                stderr: vec![],
            })
        }
    }
    for case in fixture("depth-original.json")["cases"].as_array().unwrap() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let source = root.join("source");
        let native = root.join("native");
        let output = root.join("out");
        fs::write(&source, b"source").unwrap();
        fs::write(&native, b"native").unwrap();
        let mut peer = Deep {
            depth: case["depth"].as_u64().unwrap() as usize,
            mismatch: case["mismatch"].as_bool().unwrap(),
            calls: 0,
        };
        let inputs = vec![JsonValue::parse(r#"{"op":"layout"}"#).unwrap()];
        let result = check(
            &source,
            &native,
            &output,
            "",
            "",
            &inputs,
            &mut peer,
            &mut vec![],
        );
        assert_eq!(result.is_err(), !case["error"].is_null(), "{case}");
        assert_eq!(peer.calls, 2);
        let mut files = fs::read_dir(&output)
            .unwrap()
            .map(|p| p.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(serde_json::json!(files), case["files"], "{case}");
        if let Some(bytes) = case["failure_bytes"].as_u64() {
            assert_eq!(
                fs::metadata(output.join("failure.json")).unwrap().len(),
                bytes,
                "{case}"
            );
        }
    }
}
