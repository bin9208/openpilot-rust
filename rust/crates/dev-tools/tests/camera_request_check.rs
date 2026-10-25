//! Independent unchanged request-checker generation and process contracts.
use openpilot_dev_tools::camera_request_check::{cases_json, inputs, scenarios};
use sha2::{Digest, Sha256};
fn sha(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
#[test]
fn every_original_scenario_batch_and_written_case_file_matches() {
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/camera-requests/generation-original.json"
    ))
    .unwrap();
    let cases = scenarios().unwrap();
    let rows = expected["cases"].as_array().unwrap();
    assert_eq!(cases.len(), rows.len());
    for (case, row) in cases.iter().zip(rows) {
        assert_eq!(
            case.get("name").unwrap().to_utf8().unwrap(),
            row["name"].as_str().unwrap()
        );
        assert_eq!(sha(&case.to_json().unwrap()), row["json_sha256"]);
    }
    assert_eq!(
        sha(&cases_json(&cases).unwrap()),
        expected["cases_file_sha256"]
    );
    let batches = expected["batches"].as_array().unwrap();
    assert_eq!(cases.chunks(12).len(), batches.len());
    for (batch, row) in cases.chunks(12).zip(batches) {
        let (source, native, commands, steps) = inputs(batch).unwrap();
        assert_eq!(sha(&source), row["source_sha256"]);
        assert_eq!(sha(&native), row["native_sha256"]);
        assert_eq!(commands.len(), row["commands"].as_u64().unwrap() as usize);
        assert_eq!(steps, row["steps"].as_u64().unwrap() as usize);
    }
}

use openpilot_dev_tools::{
    camera_request_check::check,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use std::{fs, path::Path, time::Duration};
struct Peer {
    mode: String,
    calls: Vec<CommandSpec>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        let lane = if self.calls.is_empty() {
            "source"
        } else {
            "native"
        };
        self.calls.push(spec.clone());
        if self.mode == format!("{lane}-spawn") || self.mode == format!("{lane}-timeout") {
            return Err("controlled process failure".into());
        }
        let mut values = vec![
            r#"{"value": 1.0}"#,
            r#"{"value": true}"#,
            r#"{"value": [2, 3]}"#,
        ];
        if ["mismatch", "failure-dir"].contains(&self.mode.as_str()) && lane == "native" {
            values[1] = r#"{"value": false}"#;
        }
        if self.mode == format!("{lane}-count") {
            values.pop();
        }
        let mut stdout = values.join("\n") + "\n";
        if self.mode == format!("{lane}-json") {
            stdout = "bad json\n".to_owned() + &stdout;
        }
        Ok(Captured {
            code: if self.mode == format!("{lane}-exit") {
                7
            } else if self.mode == format!("{lane}-signal") {
                -15
            } else {
                0
            },
            stdout: stdout.into_bytes(),
            stderr: format!("{lane} stderr\n").into_bytes(),
        })
    }
}
fn files(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    let mut result = serde_json::Map::new();
    if path.exists() {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                result.insert(
                    path.file_name().unwrap().to_str().unwrap().into(),
                    fs::read_to_string(&path).unwrap().into(),
                );
            }
        }
    }
    result
}
#[test]
fn original_process_and_failure_artifact_phases_are_preserved() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/camera-requests/process-original.json"
    ))
    .unwrap();
    for captured in corpus["cases"].as_array().unwrap() {
        let mode = captured["name"].as_str().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let source = root.join("source");
        let native = root.join("native");
        let output = root.join("output");
        fs::write(&source, b"owned source").unwrap();
        fs::write(&native, b"owned native").unwrap();
        if ["old-failure", "report-dir", "failure-dir"].contains(&mode) {
            fs::create_dir(&output).unwrap();
            if mode == "old-failure" {
                fs::write(output.join("failure.json"), "old evidence").unwrap();
            } else {
                fs::create_dir(output.join(if mode == "report-dir" {
                    "report.json"
                } else {
                    "failure.json"
                }))
                .unwrap();
            }
        }
        if mode == "missing-source" {
            fs::remove_file(&source).unwrap();
        }
        // Capture fixture uses serde's ordered input as data; recreate the original
        // typed request order from independently asserted scenario generation.
        let cases=vec![JsonValue::parse(r#"{"name":"owned","commands":[{"op":"reset","depth":18,"enabled":7,"bps":7,"ar":0,"staggered":0,"readout":22500000},{"op":"step","camera":0,"request":1,"frame":2,"timestamp":1000000000,"status":0,"first_now":1025000000,"later_now":1030000000,"failures":0,"stress":0},{"op":"step","camera":2,"request":3,"frame":4,"timestamp":2000000000,"status":0,"first_now":2025000000,"later_now":2030000000,"failures":0,"stress":0}]}"#).unwrap()];
        let mut peer = Peer {
            mode: mode.into(),
            calls: vec![],
        };
        let mut stdout = vec![];
        let result = check(
            &source,
            &native,
            &output,
            if mode == "bad-runner" { "'" } else { "" },
            "",
            &cases,
            &mut peer,
            &mut stdout,
        );
        assert_eq!(result.is_err(), !captured["error"].is_null(), "{mode}");
        assert_eq!(
            peer.calls.len(),
            captured["calls"].as_array().unwrap().len(),
            "{mode}"
        );
        for (actual, expected) in peer.calls.iter().zip(captured["calls"].as_array().unwrap()) {
            assert_eq!(actual.timeout, Duration::from_secs(90));
            assert_eq!(
                String::from_utf8(actual.input.clone().unwrap()).unwrap(),
                expected["input"].as_str().unwrap(),
                "{mode}"
            );
        }
        let norm = |text: String| text.replace(root.to_str().unwrap(), "$ROOT");
        let observed = files(&output)
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    serde_json::Value::String(norm(v.as_str().unwrap().to_owned())),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(observed, *captured["files"].as_object().unwrap(), "{mode}");
        assert_eq!(
            norm(String::from_utf8(stdout).unwrap()),
            captured["stdout"].as_str().unwrap(),
            "{mode}"
        );
    }
}
#[test]
fn original_cli_argument_admission_matches() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/camera-requests/cli-original.json")).unwrap();
    for case in original["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-requests"))
            .args(
                case["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .current_dir(temp.path())
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{case}"
        );
        assert_eq!(
            !output.stdout.is_empty(),
            case["stdout"].as_bool().unwrap(),
            "{case}"
        );
        assert_eq!(
            !output.stderr.is_empty(),
            case["stderr"].as_bool().unwrap(),
            "{case}"
        );
    }
}
#[test]
fn original_deep_parser_and_failure_serialization_boundaries_match() {
    struct Deep {
        depth: usize,
        mismatch: bool,
        calls: usize,
    }
    impl Runner for Deep {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            let number = if self.mismatch && self.calls == 1 {
                "2"
            } else {
                "1"
            };
            self.calls += 1;
            Ok(Captured {
                code: 0,
                stdout: format!(
                    "{}{}{}\n",
                    "[".repeat(self.depth),
                    number,
                    "]".repeat(self.depth)
                )
                .into_bytes(),
                stderr: vec![],
            })
        }
    }
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/camera-requests/depth-original.json")).unwrap();
    for case in expected["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let source = root.join("source");
        let native = root.join("native");
        let output = root.join("out");
        fs::write(&source, b"a").unwrap();
        fs::write(&native, b"b").unwrap();
        let cases=vec![JsonValue::parse(r#"{"name":"deep","commands":[{"op":"reset","depth":18,"enabled":7,"bps":7,"ar":0,"staggered":0,"readout":22500000}]}"#).unwrap()];
        let mut runner = Deep {
            depth: case["depth"].as_u64().unwrap() as usize,
            mismatch: case["mismatch"].as_bool().unwrap(),
            calls: 0,
        };
        let result = check(
            &source,
            &native,
            &output,
            "",
            "",
            &cases,
            &mut runner,
            &mut vec![],
        );
        assert_eq!(result.is_err(), !case["error"].is_null(), "{case}");
        let mut observed = fs::read_dir(&output)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        observed.sort();
        assert_eq!(
            serde_json::to_value(observed).unwrap(),
            case["files"],
            "{case}"
        );
        if let Some(bytes) = case["failure_bytes"].as_u64() {
            assert_eq!(
                fs::metadata(output.join("failure.json")).unwrap().len(),
                bytes,
                "{case}"
            );
        }
    }
}
#[cfg(unix)]
#[test]
fn real_cli_preserves_raw_runner_and_output_paths_without_python() {
    use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let runner = root.join(std::ffi::OsString::from_vec(b"runner\xff".to_vec()));
    fs::write(
        &runner,
        "#!/bin/sh\nwhile IFS= read -r line; do printf 'null\\n'; done\n",
    )
    .unwrap();
    fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.join("source"), b"source").unwrap();
    fs::write(root.join("native"), b"native").unwrap();
    let out = std::ffi::OsString::from_vec(b"out\xff".to_vec());
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-requests"))
        .current_dir(root)
        .env("PATH", "/nonexistent")
        .args(["--reference", "./source", "--binary", "native", "--output"])
        .arg(&out)
        .arg("--reference-runner")
        .arg(&runner)
        .arg("--runner")
        .arg(&runner)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report = fs::read_to_string(root.join(out).join("report.json")).unwrap();
    assert!(report.contains("\\udcff"));
    let report: serde_json::Value = serde_json::from_str(&report.replace("\\udcff", "?")).unwrap();
    assert_eq!(report["source_command"][1], "source");
    assert_eq!(report["native_command"][1], "native");
    assert_eq!(report["scenarios"], 512);
    assert_eq!(report["steps"], 72493);
}
