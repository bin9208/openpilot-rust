//! Independent ISP lifecycle scenarios, ownership normalization and artifact phases.
use openpilot_dev_tools::{
    camera_isp_lifecycle_check::{check, scenarios},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use std::{ffi::OsString, fs, path::Path, time::Duration};
fn fixture(name: &str) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/camera-isp-lifecycle-check")
                .join(name),
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn all30_original_scenarios_match_argument_and_environment_order() {
    let original = fixture("process-original.json");
    let cases = scenarios(false);
    assert_eq!(cases.len(), 30);
    let first = &original["cases"][0];
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(case.name, original["case_names"][index]);
        assert_eq!(
            serde_json::json!(case.arguments),
            original["case_argv"][index]
        );
        assert_eq!(
            serde_json::json!(case
                .environment
                .iter()
                .cloned()
                .collect::<std::collections::BTreeMap<_, _>>()),
            original["case_overrides"][index]
        );
        let expected = first["calls"][index * 2]["environment"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| {
                let k = v[0].as_str().unwrap();
                if k.starts_with("CK_") && k != "CK_AMBIENT" && k != "CK_TRACE" {
                    Some((k.to_owned(), v[1].as_str().unwrap().to_owned()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(case.environment, expected);
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
        assert_eq!(spec.timeout, Duration::from_secs(45));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(spec.disable_core_dump);
        assert!(!spec.merge_output && !spec.inherit_output && spec.inherit_stdin);
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
            let folder = output.join("sensor-1-mode-0-depth-1/source");
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
            &scenarios(mode == "smoke"),
            &environment,
            &mut peer,
            &mut printed,
        );
        if case["error"]["type"] == "SystemExit" {
            assert_eq!(
                result.as_ref().ok().copied(),
                Some(case["exit"].as_u64().unwrap() as usize),
                "{mode}: {result:?}"
            );
        } else {
            assert!(result.is_err(), "{mode}: {result:?}");
        }
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
        let r = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-isp-lifecycle"))
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
fn raw_qemu_output_paths_and_inherited_stdin_match_original() {
    use std::{
        ffi::OsString,
        io::Write,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
        process::Stdio,
    };
    let expected = fixture("raw-stdin-original.json");
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let peer = root.join(OsString::from_vec(b"qemu-\xff".to_vec()));
    let out = root.join(OsString::from_vec(b"out-\xff".to_vec()));
    for (p, bytes) in [
        (root.join("source"), b"source".as_slice()),
        (root.join("native"), b"native"),
        (root.join("fixture"), b"fixture"),
    ] {
        fs::write(p, bytes).unwrap();
    }
    fs::write(
        &peer,
        "#!/bin/sh\nIFS= read -r token || token=EOF\nprintf '{\"stdin\":\"%s\"}\\n' \"$token\"\n",
    )
    .unwrap();
    fs::set_permissions(&peer, fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-isp-lifecycle"))
        .current_dir(root)
        .env("PATH", "/nonexistent")
        .args([
            "--source",
            "./source",
            "--native",
            "native",
            "--fixture",
            "fixture",
            "--output",
        ])
        .arg(&out)
        .arg("--qemu")
        .arg(&peer)
        .args(["--sysroot", "/owned"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&b"token\n".repeat(60))
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(
        result.status.code(),
        Some(expected["exit"].as_i64().unwrap() as i32),
        "{:?}",
        result.stderr
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        expected["stdout"]
    );
    assert_eq!(
        fs::read_to_string(out.join("sensor-1-mode-0-depth-1/source/stdout")).unwrap(),
        expected["source_stdout"]
    );
    assert!(out.join("sensor-1-mode-2-depth-1/source/run.json").exists());
    assert_eq!(
        walk_count(&out),
        expected["files"].as_u64().unwrap() as usize
    );
    fn walk_count(path: &Path) -> usize {
        fs::read_dir(path)
            .unwrap()
            .map(|v| {
                let p = v.unwrap().path();
                1 + if p.is_dir() { walk_count(&p) } else { 0 }
            })
            .sum()
    }
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("report.json")).unwrap()).unwrap();
    let original: serde_json::Value =
        serde_json::from_str(expected["report_json"].as_str().unwrap()).unwrap();
    assert_eq!(report["cases"], original["cases"]);
    assert_eq!(report["failures"], original["failures"]);
    assert_eq!(report["binaries"], original["binaries"]);
}

#[test]
fn original_allocation_ownership_normalization_and_error_classes() {
    use openpilot_dev_tools::camera_isp_lifecycle_check::normalize;
    use openpilot_log_format::JsonValue;
    for case in fixture("normalize-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let source = JsonValue::parse(case["source_json"].as_str().unwrap()).unwrap();
        let native = JsonValue::parse(case["native_json"].as_str().unwrap()).unwrap();
        let actual = normalize(&source, &native);
        let assertion = case["exception"] == "AssertionError";
        assert_eq!(
            actual.is_err(),
            !case["exception"].is_null() && !assertion,
            "{case}: {}",
            actual
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_default()
        );
        if let Ok((cleaned, ownership, error)) = actual {
            assert_eq!(error.as_deref(), case["error"].as_str(), "{case}");
            if !assertion {
                assert_eq!(
                    cleaned.to_json().unwrap(),
                    JsonValue::parse(case["cleaned_json"].as_str().unwrap())
                        .unwrap()
                        .to_json()
                        .unwrap(),
                    "{case}"
                );
                assert_eq!(
                    ownership.to_json().unwrap(),
                    JsonValue::parse(case["ownership_json"].as_str().unwrap())
                        .unwrap()
                        .to_json()
                        .unwrap(),
                    "{case}"
                );
            }
        }
    }
}
