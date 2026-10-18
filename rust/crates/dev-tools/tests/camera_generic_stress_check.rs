//! Independently captured original process protocol, failure phases and artifacts.
#![cfg(feature = "camera-sensor-native")]
use openpilot_dev_tools::{
    camera_generic_stress_check::{check, Options},
    camera_stress_check::scenarios,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};
#[test]
fn original_policy_scenarios_preserve_inputs_and_expected_boundaries() {
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/camera-generic-stress/cases-original.json"
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
fn original_process_cases() {
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/camera-generic-stress/process-original.json"
    ))
    .unwrap();
    for case in original["cases"].as_array().unwrap() {
        let root = tempfile::tempdir().unwrap();
        for name in ["source", "native", "parser", "fixture"] {
            fs::write(root.path().join(name), name).unwrap();
        }
        let output = root.path().join("out");
        if case["name"] == "output-exists" {
            fs::create_dir(&output).unwrap();
        }

        let opt = Options {
            reference: root.path().join("source"),
            native: root.path().join("native"),
            parser: root.path().join("parser"),
            fixture: root.path().join("fixture"),
            output: output.clone(),
            runner: if case["name"] == "runner" {
                vec![root.path().join("runner").into_os_string(), "nested".into()]
            } else {
                vec![]
            },
            profile: if case["name"] == "profile" {
                "custom".into()
            } else {
                "host".into()
            },
        };
        let mut peer = Peer {
            case,
            root: root.path(),
            calls: 0,
        };
        let mut times = [100u128, 200].into_iter();
        let mut clock = || Ok(times.next().unwrap());
        let mut printed = Vec::new();
        let result = check(&opt, &mut peer, &mut clock, &mut printed);
        assert_eq!(String::from_utf8(printed).unwrap(), case["stdout"]);
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
    original_process_cases();
}
#[test]
fn original_nargs_runner_and_cli_failure_artifacts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/camera-generic-stress/cli-original.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("out");
        let argv = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                a.as_str()
                    .unwrap()
                    .replace("$OUT", output.to_str().unwrap())
            })
            .collect::<Vec<_>>();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-stress"))
            .args(argv)
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(
            result.status.code().unwrap() as i64,
            case["exit"].as_i64().unwrap(),
            "{}",
            case["name"]
        );
        if let Some(stdout) = case["stdout"].as_str() {
            assert_eq!(result.stdout, stdout.as_bytes());
        }
        assert_eq!(output.exists(), case["output_exists"].as_bool().unwrap());
        let files = if output.exists() {
            fs::read_dir(&output)
                .unwrap()
                .map(|e| {
                    let p = e.unwrap().path();
                    (
                        p.file_name().unwrap().to_str().unwrap().into(),
                        Value::String(
                            fs::read_to_string(p)
                                .unwrap()
                                .replace(output.to_str().unwrap(), "$OUT"),
                        ),
                    )
                })
                .collect::<serde_json::Map<_, _>>()
        } else {
            serde_json::Map::new()
        };
        assert_eq!(Value::Object(files), case["files"], "{}", case["name"]);
    }
}
#[test]
fn original_raw_runner_bytes_remain_single_arguments_and_preserve_receipts() {
    use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/camera-generic-stress/raw-cli-original.json"
    ))
    .unwrap();
    let decode = |s: &str| {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>()
    };
    for case in fixture["cases"].as_array().unwrap() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("out");
        let peer = root.path().join(std::ffi::OsString::from_vec(decode(
            case["peer_hex"].as_str().unwrap(),
        )));
        fs::write(&peer, b"#!/bin/sh\nprintf \"{}\\n\"\n").unwrap();
        fs::set_permissions(&peer, fs::Permissions::from_mode(0o755)).unwrap();
        let mut raw = decode(case["runner_hex"].as_str().unwrap());
        if raw.starts_with(b"$ROOT") {
            let mut replacement = root.path().as_os_str().as_encoded_bytes().to_vec();
            replacement.extend_from_slice(&raw[5..]);
            raw = replacement;
        }
        let mut args: Vec<std::ffi::OsString> = [
            "--reference",
            "missing",
            "--native",
            "missing",
            "--parser",
            "missing",
            "--fixture",
            "missing",
            "--output",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        args.push(output.as_os_str().to_owned());
        if case["equals"] == true {
            let mut flag = b"--runner=".to_vec();
            flag.extend(raw);
            args.push(std::ffi::OsString::from_vec(flag));
        } else {
            args.push("--runner".into());
            args.push(std::ffi::OsString::from_vec(raw));
        }
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-stress"))
            .args(args)
            .env("PATH", root.path())
            .current_dir(root.path())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code().unwrap() as i64,
            case["exit"].as_i64().unwrap(),
            "{}",
            case["name"]
        );
        let files = fs::read_dir(&output)
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                (
                    p.file_name().unwrap().to_str().unwrap().into(),
                    Value::String(
                        fs::read_to_string(p)
                            .unwrap()
                            .replace(root.path().to_str().unwrap(), "$ROOT"),
                    ),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(Value::Object(files), case["files"], "{}", case["name"]);
    }
}
