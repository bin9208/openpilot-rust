//! Independent kernel scenarios, Python trace normalization and artifact phases.
use openpilot_dev_tools::{
    camera_kernel_check::{baseline, check, normalize, scenarios, Scenario},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{ffi::OsString, fs, path::Path, time::Duration};
fn fixture(name: &str) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/camera-kernel-check")
                .join(name),
        )
        .unwrap(),
    )
    .unwrap()
}
fn scenario(value: &JsonValue) -> Scenario {
    let JsonView::Array(parts) = value.view() else {
        panic!("scenario array");
    };
    let JsonView::Object(environment) = parts[3].view() else {
        panic!("scenario environment");
    };
    Scenario {
        name: parts[0].to_utf8().unwrap(),
        mode: parts[1].to_utf8().unwrap(),
        seed: parts[2].to_json().unwrap().parse().unwrap(),
        environment: environment
            .into_iter()
            .map(|(key, value)| {
                (
                    key.iter()
                        .copied()
                        .map(char::from_u32)
                        .collect::<Option<String>>()
                        .unwrap(),
                    value.to_utf8().unwrap(),
                )
            })
            .collect(),
    }
}
#[test]
fn all334_original_scenarios_baseline_and_trace_normalization() {
    let original = fixture("generation-original.json");
    let cases = scenarios();
    assert_eq!(cases.len(), 334);
    // Compare each original scalar/environment independently; fixture objects are
    // semantically unordered, while receipt ordering is separately asserted below.
    for (actual, expected) in cases.iter().zip(original["scenarios"].as_array().unwrap()) {
        assert_eq!(actual.name, expected[0]);
        assert_eq!(actual.mode, expected[1]);
        assert_eq!(actual.seed, expected[2].as_u64().unwrap());
        assert_eq!(
            serde_json::json!(actual
                .environment
                .iter()
                .cloned()
                .collect::<std::collections::BTreeMap<_, _>>()),
            expected[3]
        );
    }
    let actual = baseline(&cases);
    assert_eq!(actual.len(), 25);
    for (a, b) in actual.iter().zip(original["baseline"].as_array().unwrap()) {
        assert_eq!(a.name, b[0]);
    }
    for case in original["normalize"].as_array().unwrap() {
        let input = JsonValue::parse(case["input_json"].as_str().unwrap()).unwrap();
        let result = normalize(&input);
        assert_eq!(result.is_err(), !case["error"].is_null(), "{case}");
        if let Ok(value) = result {
            assert_eq!(
                value.to_json().unwrap(),
                JsonValue::parse(case["output_json"].as_str().unwrap())
                    .unwrap()
                    .to_json()
                    .unwrap()
            );
        }
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
        self.calls += 1;
        let mode = self.case["name"].as_str().unwrap();
        let lane = if self.calls % 2 == 1 {
            "source"
        } else {
            "native"
        };
        assert_eq!(spec.timeout, Duration::from_secs(30));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(spec.disable_core_dump);
        assert!(!spec.merge_output && !spec.inherit_output);
        assert!(spec.inherit_stdin);
        assert!(spec.input.is_none());
        let argv = spec
            .effective_argv()
            .iter()
            .map(|v| {
                v.to_string_lossy()
                    .replace(self.root.to_str().unwrap(), "$ROOT")
            })
            .collect::<Vec<_>>();
        assert_eq!(serde_json::json!(argv), expected["argv"]);
        let environment = spec.effective_env();
        for entry in expected["environment"].as_array().unwrap() {
            let key = entry[0].as_str().unwrap();
            let value = entry[1]
                .as_str()
                .unwrap()
                .replace("$ROOT", self.root.to_str().unwrap());
            assert_eq!(
                environment.get(std::ffi::OsStr::new(key)),
                Some(&OsString::from(value))
            );
        }
        let trace = Path::new(environment.get(std::ffi::OsStr::new("CK_TRACE")).unwrap());
        assert!(!trace.exists());
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
        if let Some(text) = expected["trace_text"].as_str() {
            fs::write(trace, text)?;
        }
        Ok(Captured {
            code: expected["code"].as_i64().unwrap() as i32,
            stdout: expected["stdout"].as_str().unwrap().as_bytes().to_vec(),
            stderr: expected["stderr"].as_str().unwrap().as_bytes().to_vec(),
        })
    }
}
#[test]
fn original_process_environment_sanitizer_failure_and_artifact_order() {
    let original = fixture("process-original.json");
    let cases_json = JsonValue::parse(original["scenarios_json"].as_str().unwrap()).unwrap();
    let JsonView::Array(cases) = cases_json.view() else {
        panic!("cases array");
    };
    let cases = cases.iter().map(scenario).collect::<Vec<_>>();
    let environment = original["environment"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                OsString::from(v[0].as_str().unwrap()),
                OsString::from(v[1].as_str().unwrap()),
            )
        })
        .collect::<Vec<_>>();
    for case in original["cases"].as_array().unwrap() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let source = root.join("source");
        let native = root.join("native");
        let library = root.join("fixture");
        let output = root.join("out");
        for (path, bytes) in [
            (&source, b"source".as_slice()),
            (&native, b"native"),
            (&library, b"fixture"),
        ] {
            fs::write(path, bytes).unwrap();
        }
        let mode = case["name"].as_str().unwrap();
        if ["missing-source", "missing-native", "missing-fixture"].contains(&mode) {
            fs::remove_file(if mode == "missing-source" {
                &source
            } else if mode == "missing-native" {
                &native
            } else {
                &library
            })
            .unwrap();
        }
        if mode == "output-file" {
            fs::write(&output, "old output\n").unwrap();
        }
        if [
            "old-trace",
            "trace-directory",
            "stdout-directory",
            "stderr-directory",
            "run-directory",
            "report-directory",
        ]
        .contains(&mode)
        {
            let folder = output.join("owned-open/source");
            fs::create_dir_all(&folder).unwrap();
            if mode == "old-trace" {
                fs::write(folder.join("trace.jsonl"), "obsolete trace\n").unwrap();
            } else if mode == "report-directory" {
                fs::create_dir(output.join("report.json")).unwrap();
            } else {
                fs::create_dir(folder.join(match mode {
                    "trace-directory" => "trace.jsonl",
                    "stdout-directory" => "stdout",
                    "stderr-directory" => "stderr",
                    "run-directory" => "run.json",
                    _ => panic!(),
                }))
                .unwrap();
            }
        }
        let qemu = if mode.starts_with("qemu") {
            Some(Path::new(if mode == "qemu-empty" {
                "."
            } else {
                "/owned/qemu"
            }))
        } else {
            None
        };
        let sysroot = if mode.starts_with("qemu") && mode != "qemu-missing-sysroot" {
            Some(Path::new("/owned/sysroot"))
        } else {
            None
        };
        let mut peer = Peer {
            case,
            root,
            calls: 0,
        };
        let mut printed = vec![];
        let result = check(
            &source,
            &native,
            &library,
            &output,
            qemu,
            sysroot,
            mode == "baseline",
            &cases,
            &environment,
            &mut peer,
            &mut printed,
        );
        let success = case["exit"] == 0;
        assert_eq!(matches!(result, Ok(0)), success, "{mode}: {result:?}");
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
            case["output_kind"]
        );
        fn files(
            folder: &Path,
            root: &Path,
            out: &mut serde_json::Map<String, serde_json::Value>,
            temp: &Path,
        ) {
            if !folder.is_dir() {
                return;
            }
            for entry in fs::read_dir(folder).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if ["stdout", "stderr", "trace.jsonl", "run.json", "report.json"]
                        .contains(&path.file_name().unwrap().to_str().unwrap())
                    {
                        out.insert(
                            path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                            serde_json::json!("directory"),
                        );
                    }
                    files(&path, root, out, temp);
                } else {
                    out.insert(
                        path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                        serde_json::json!(fs::read_to_string(&path)
                            .unwrap()
                            .replace(temp.to_str().unwrap(), "$ROOT")),
                    );
                }
            }
        }
        let mut actual = serde_json::Map::new();
        files(&output, &output, &mut actual, root);
        assert_eq!(serde_json::Value::Object(actual), case["files"], "{mode}");
    }
}
#[test]
fn actual_native_cli_argument_contract() {
    for case in fixture("cli-original.json")["cases"].as_array().unwrap() {
        let tmp = tempfile::tempdir().unwrap();
        let r = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-kernel"))
            .args(
                case["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert_eq!(
            r.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{case}"
        );
        assert_eq!(!r.stdout.is_empty(), case["stdout"]);
        assert_eq!(!r.stderr.is_empty(), case["stderr"]);
    }
}

#[cfg(unix)]
#[test]
fn actual_cli_inherits_stdin_for_each_source_and_native_child() {
    use std::{io::Write, os::unix::fs::PermissionsExt, process::Stdio};
    let expected = fixture("stdin-original.json");
    assert_eq!(
        expected["source_sha256"],
        "23c6eed1cdf11d9045e2ee63e8033170845d2fc24b71912c391655ca8be0f72c"
    );
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for name in ["source", "native", "fixture"] {
        fs::write(root.join(name), "owned bytes").unwrap();
    }
    let peer = root.join("qemu");
    fs::write(
        &peer,
        "#!/bin/sh\nIFS= read -r token || token=EOF\nprintf '{\"stdin\":\"%s\"}\\n' \"$token\"\n",
    )
    .unwrap();
    fs::set_permissions(&peer, fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-kernel"))
        .current_dir(root)
        .env("PATH", "/nonexistent")
        .args([
            "--source",
            "source",
            "--native",
            "native",
            "--fixture",
            "fixture",
            "--output",
            "out",
            "--qemu",
        ])
        .arg(&peer)
        .args(["--sysroot", "/owned", "--baseline-only"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&b"token\n".repeat(50))
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(
        result.status.code(),
        Some(expected["exit"].as_i64().unwrap() as i32)
    );
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        expected["stderr"]
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("out/report.json")).unwrap()).unwrap();
    assert_eq!(report["cases"], expected["cases"]);
    assert_eq!(report["failures"], expected["failures"]);
    // Null stdin makes both peers return EOF and the comparison still passes.
    // Inspect every lane so parity alone cannot conceal the lost input.
    for case in expected["cases"].as_array().unwrap() {
        for lane in ["source", "native"] {
            assert_eq!(
                fs::read_to_string(
                    root.join("out")
                        .join(case["name"].as_str().unwrap())
                        .join(lane)
                        .join("stdout")
                )
                .unwrap(),
                expected["source_stdout"]
            );
        }
    }
}
