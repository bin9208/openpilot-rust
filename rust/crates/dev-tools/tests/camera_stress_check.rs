//! Independently captured original process protocol, failure phases and artifacts.
#![cfg(feature = "camera-sensor-native")]
use openpilot_dev_tools::{
    camera_stress_check::{check, scenarios, Options},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};
#[test]
fn original_policy_scenarios_preserve_inputs_and_expected_boundaries() {
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/camera-stress-check/cases-original.json"
    ))
    .unwrap();
    assert_eq!(
        serde_json::to_value(scenarios()).unwrap(),
        original["cases"]
    );
}
struct Peer<'a> {
    case: &'a Value,
    root: &'a Path,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let expected = &self.case["calls"][self.calls];
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::from_secs(10));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(!spec.inherit_output && !spec.merge_output);
        assert_eq!(spec.inherit_stdin, expected["input"].is_null());
        let argv = spec
            .effective_argv()
            .iter()
            .map(|v| {
                v.to_str()
                    .unwrap()
                    .replace(self.root.to_str().unwrap(), "$ROOT")
            })
            .collect::<Vec<_>>();
        assert_eq!(serde_json::to_value(argv).unwrap(), expected["argv"]);
        assert_eq!(
            spec.input.as_deref(),
            expected["input"].as_str().map(str::as_bytes)
        );
        let mut env = serde_json::Map::new();
        for (k, v) in spec.os_env.as_ref().unwrap() {
            let k = k.to_str().unwrap();
            if k.starts_with("STRESS_")
                || k.starts_with("SPECTRA_ERROR_")
                || k.starts_with("LD_PRELOAD")
            {
                env.insert(
                    k.into(),
                    v.to_str()
                        .unwrap()
                        .replace(self.root.to_str().unwrap(), "$ROOT")
                        .into(),
                );
            }
        }
        assert_eq!(Value::Object(env), expected["environment"]);
        let Some(stdout) = expected["stdout"].as_str() else {
            return Err("original child boundary failure".into());
        };
        if let Some(trace) = spec
            .os_env
            .as_ref()
            .unwrap()
            .get(std::ffi::OsStr::new("STRESS_TRACE"))
        {
            let rows = expected["trace"].as_array().unwrap();
            if !rows.is_empty() {
                fs::write(trace, expected["trace_text"].as_str().unwrap())?;
            }
        }
        Ok(Captured {
            code: expected["exit"].as_i64().unwrap() as i32,
            stdout: stdout.as_bytes().to_vec(),
            stderr: expected["stderr"].as_str().unwrap().as_bytes().to_vec(),
        })
    }
}
fn original_process_cases(errno_controls: bool) {
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/camera-stress-check/process-original.json"
    ))
    .unwrap();
    for case in original["cases"].as_array().unwrap() {
        let errno_control = case["name"].as_str().unwrap().starts_with("parser-errno-")
            && !["parser-errno-zero", "parser-errno-wrong"]
                .contains(&case["name"].as_str().unwrap());
        if errno_control != errno_controls {
            continue;
        }
        let root = tempfile::tempdir().unwrap();
        for name in ["source", "native", "parser", "fixture"] {
            fs::write(root.path().join(name), name).unwrap();
        }
        let output = root.path().join("out");
        if case["name"] == "output-exists" {
            fs::create_dir(&output).unwrap();
        }
        let qemu = root.path().join("qemu");
        let sysroot = root.path().join("sysroot");
        let opt = Options {
            reference: root.path().join("source"),
            native: root.path().join("native"),
            parser: root.path().join("parser"),
            fixture: root.path().join("fixture"),
            output: output.clone(),
            qemu: case["name"]
                .as_str()
                .unwrap()
                .starts_with("qemu")
                .then_some(qemu),
            sysroot: (case["name"] == "qemu").then_some(sysroot),
        };
        let mut peer = Peer {
            case,
            root: root.path(),
            calls: 0,
        };
        let mut times = [100u128, 200].into_iter();
        let mut clock = || Ok(times.next().unwrap());
        let result = check(&opt, &mut peer, &mut clock, &mut Vec::new());
        assert_eq!(
            result.is_ok(),
            case["success"].as_bool().unwrap(),
            "{}: {result:?}",
            case["name"]
        );
        assert_eq!(
            peer.calls,
            case["calls"].as_array().unwrap().len(),
            "{}",
            case["name"]
        );
        let mut actual = serde_json::Map::new();
        if output.exists() {
            for entry in fs::read_dir(&output).unwrap() {
                let path = entry.unwrap().path();
                let text = fs::read_to_string(&path)
                    .unwrap()
                    .replace(root.path().to_str().unwrap(), "$ROOT");
                actual.insert(path.file_name().unwrap().to_str().unwrap().into(),serde_json::json!({"sha256":format!("{:x}",Sha256::digest(text.as_bytes())),"bytes":text.len()}));
            }
        }
        assert_eq!(
            Value::Object(actual),
            case["files"],
            "{} artifact preservation",
            case["name"]
        );
    }
}

#[test]
fn original_process_failures_and_partial_artifacts_are_preserved() {
    original_process_cases(false);
}

#[test]
fn original_errno_python_truthiness_and_fallback_preserve_artifacts() {
    original_process_cases(true);
}

#[test]
fn original_cli_admission_and_side_effects_without_python() {
    let cases: Value = serde_json::from_str(include_str!(
        "fixtures/camera-stress-check/cli-original.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("out");
        let args = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap()
                    .replace("$OUT", output.to_str().unwrap())
            })
            .collect::<Vec<_>>();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-runtime-stress"))
            .args(args)
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{}",
            case["name"]
        );
        assert_eq!(
            output.exists(),
            case["output_exists"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        let mut files = if output.exists() {
            fs::read_dir(output)
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        files.sort();
        assert_eq!(
            serde_json::to_value(files).unwrap(),
            case["files"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn original_cli_lexical_paths_preserve_path_lookup_and_receipts() {
    use std::os::unix::fs::PermissionsExt;
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/camera-stress-check/cli-paths-original.json"
    ))
    .unwrap();
    for case in original["cases"].as_array().unwrap() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("bin")).unwrap();
        fs::create_dir(root.path().join("nested")).unwrap();
        for (name, marker) in [("qemu", "cwd"), ("bin/qemu", "PATH")] {
            let path = root.path().join(name);
            fs::write(
                &path,
                format!("#!/bin/sh\nprintf '%s\\n' '{{\"origin\":\"{marker}\"}}'\n"),
            )
            .unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let expand = |s: &str| s.replace("$ROOT", root.path().to_str().unwrap());
        let args = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| expand(v.as_str().unwrap()))
            .collect::<Vec<_>>();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-runtime-stress"))
            .args(args)
            .current_dir(root.path())
            .env("PATH", expand(case["path"].as_str().unwrap()))
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{}",
            case["name"]
        );
        assert_eq!(result.stdout, case["stdout"].as_str().unwrap().as_bytes());
        let output = root.path().join("out");
        assert_eq!(output.exists(), case["output_exists"].as_bool().unwrap());
        let mut files = serde_json::Map::new();
        if output.exists() {
            for entry in fs::read_dir(output).unwrap() {
                let path = entry.unwrap().path();
                let text = fs::read_to_string(&path)
                    .unwrap()
                    .replace(root.path().to_str().unwrap(), "$ROOT");
                files.insert(
                    path.file_name().unwrap().to_str().unwrap().into(),
                    text.into(),
                );
            }
        }
        assert_eq!(Value::Object(files), case["files"], "{}", case["name"]);
    }
}
