//! Independent camera lifecycle scenarios, resource cleanup and artifact phases.
use openpilot_dev_tools::{
    camera_lifecycle_check::{check, scenarios},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use std::{ffi::OsString, fs, path::Path, time::Duration};
fn fixture(name: &str) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/camera-lifecycle-check")
                .join(name),
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn all32_original_scenarios_match_argument_and_environment_order() {
    let original = fixture("process-original.json");
    let cases = scenarios(false, false, &[]).unwrap();
    assert_eq!(cases.len(), 32);
    let first = &original["cases"][0];
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(case.name, original["pass_cases"][index]["name"]);
        assert_eq!(
            serde_json::json!(case.arguments),
            original["pass_cases"][index]["arguments"]
        );
        assert_eq!(
            serde_json::json!(case
                .environment
                .iter()
                .cloned()
                .collect::<std::collections::BTreeMap<_, _>>()),
            original["pass_cases"][index]["environment"]
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
    namespaces: Vec<String>,
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
        assert_eq!(spec.timeout, Duration::from_secs(90));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(spec.disable_core_dump);
        assert_eq!(
            spec.env_remove,
            ["CEREAL_FAKE", "SPECTRA_ERROR_PROB", "SPECTRA_ERROR_DT"].map(OsString::from)
        );
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
        let prefix = environment
            .get(std::ffi::OsStr::new("OPENPILOT_PREFIX"))
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(Path::new("/dev/shm")
            .join(format!("msgq_{prefix}"))
            .is_dir());
        assert!(!self.namespaces.contains(&prefix));
        self.namespaces.push(prefix.clone());
        for removed in ["CEREAL_FAKE", "SPECTRA_ERROR_PROB", "SPECTRA_ERROR_DT"] {
            assert!(!environment.contains_key(std::ffi::OsStr::new(removed)));
        }
        for entry in expected["environment"].as_array().unwrap() {
            let key = entry[0].as_str().unwrap();
            let value = entry[1]
                .as_str()
                .unwrap()
                .replace("$ROOT", self.root.to_str().unwrap())
                .replace("$NAMESPACE", &prefix);
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
    for case in original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"] != "no-selected")
    {
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
        if mode == "existing-output" {
            fs::create_dir(&output).unwrap();
        }
        if mode == "existing-source-folder" {
            fs::create_dir_all(output.join("ar-mode-0-depth-1/source")).unwrap();
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
            namespaces: vec![],
        };
        let mut printed = vec![];
        let result = check(
            &source,
            &native,
            &library,
            &output,
            qemu,
            sysroot,
            &scenarios(
                mode == "smoke" || mode == "cleanup-smoke",
                mode == "cleanup" || mode == "cleanup-smoke",
                &if mode == "selected" {
                    vec![
                        "error-2-sync:1".into(),
                        "ar-mode-0-depth-1".into(),
                        "ar-mode-0-depth-1".into(),
                    ]
                } else if mode == "no-selected" {
                    vec!["unknown".into()]
                } else {
                    vec![]
                },
            )
            .unwrap_or_default(),
            &environment,
            if matches!(mode, "disk-low" | "disk-boundary") {
                let free = case["free"].as_u64().unwrap();
                assert!(((26u64 << 30) - 1..=(26u64 << 30)).contains(&free));
                free - (25u64 << 30)
            } else {
                case["free"].as_u64().unwrap()
            },
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
        for prefix in &peer.namespaces {
            assert!(!Path::new("/dev/shm")
                .join(format!("msgq_{prefix}"))
                .exists());
        }
        let mut actual = serde_json::Map::new();
        files(&output, &output, &mut actual, root);
        for value in actual.values_mut() {
            if let Some(text) = value.as_str() {
                let mut text = text.to_owned();
                for prefix in &peer.namespaces {
                    text = text.replace(prefix, "$NAMESPACE");
                }
                *value = serde_json::Value::String(text);
            }
        }
        assert_eq!(serde_json::Value::Object(actual), case["files"], "{mode}");
    }
}
#[test]
fn actual_native_cli_argument_contract() {
    for case in fixture("cli-original.json")["cases"].as_array().unwrap() {
        let tmp = tempfile::tempdir().unwrap();
        let r = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-camera-lifecycle"))
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
        assert_eq!(tmp.path().join("out").exists(), case["output_exists"]);
    }
}

#[test]
fn original_smoke_cleanup_and_repeated_case_selection() {
    let original = fixture("process-original.json");
    for (key, smoke, cleanup, selected) in [
        ("smoke_cases", true, false, vec![]),
        ("cleanup_cases", false, true, vec![]),
        ("cleanup_smoke_cases", true, true, vec![]),
        (
            "selected_cases",
            false,
            false,
            vec![
                OsString::from("error-2-sync:1"),
                "ar-mode-0-depth-1".into(),
                "ar-mode-0-depth-1".into(),
            ],
        ),
    ] {
        let actual = scenarios(smoke, cleanup, &selected).unwrap();
        assert_eq!(
            actual.len(),
            original[key].as_array().unwrap().len(),
            "{key}"
        );
        for (actual, expected) in actual.iter().zip(original[key].as_array().unwrap()) {
            assert_eq!(actual.name, expected["name"]);
            assert_eq!(serde_json::json!(actual.arguments), expected["arguments"]);
            assert_eq!(
                serde_json::json!(actual
                    .environment
                    .iter()
                    .cloned()
                    .collect::<std::collections::BTreeMap<_, _>>()),
                expected["environment"]
            );
        }
    }
    assert!(scenarios(false, false, &["unknown".into()]).is_err());
}
#[test]
fn original_image_identity_f32_and_resource_cleanup_helper_semantics() {
    use openpilot_dev_tools::camera_lifecycle_check::{
        audit, images, normalize_disabled, result_value,
    };
    use openpilot_log_format::{JsonValue, JsonView};
    for case in fixture("helpers-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let arguments = JsonValue::parse(case["arguments_json"].as_str().unwrap()).unwrap();
        let JsonView::Array(args) = arguments.view() else {
            panic!()
        };
        let result = match case["function"].as_str().unwrap() {
            "images" => images(&args[0]).and_then(|(a, b)| {
                Ok(JsonValue::parse(&format!(
                    "[{}, {}]",
                    a.to_json()?,
                    b.to_json()?
                ))?)
            }),
            "result_value" => result_value(
                &args[0].to_utf8().unwrap(),
                args[1].to_json().unwrap() == "true",
            ),
            "audit" => audit(&args[0]),
            "normalize_disabled" => {
                normalize_disabled(&args[0], &args[1], &args[2].to_utf8().unwrap()).and_then(
                    |(a, b)| {
                        Ok(JsonValue::parse(&format!(
                            "[{}, {}]",
                            a.to_json()?,
                            b.to_json()?
                        ))?)
                    },
                )
            }
            _ => panic!("unknown helper"),
        };
        if case["error"].is_null() {
            let value = result.unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
            assert_eq!(
                value.to_json().unwrap(),
                JsonValue::parse(case["value_json"].as_str().unwrap())
                    .unwrap()
                    .to_json()
                    .unwrap(),
                "{}",
                case["name"]
            );
        } else {
            let error = match result {
                Ok(_) => panic!("unexpected success: {}", case["name"]),
                Err(e) => e,
            };
            assert_eq!(
                error.to_string(),
                case["error"]["message"],
                "{}",
                case["name"]
            );
        }
    }
}

#[test]
fn original_failure_artifact_paths_preserve_selectors_and_receipts() {
    use openpilot_dev_tools::camera_lifecycle_check::Scenario;
    struct PathPeer {
        prefixes: Vec<String>,
    }
    impl Runner for PathPeer {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            let env = spec.effective_env();
            assert_eq!(
                env.get(std::ffi::OsStr::new("CK_FAIL_OP")),
                Some(&OsString::from("camera:271"))
            );
            self.prefixes.push(
                env.get(std::ffi::OsStr::new("OPENPILOT_PREFIX"))
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .into(),
            );
            Ok(Captured {
                code: 0,
                stdout: b"{}".to_vec(),
                stderr: vec![],
            })
        }
    }
    for original in fixture("paths-original.json")["cases"].as_array().unwrap() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for name in ["source", "native", "fixture"] {
            fs::write(root.join(name), name).unwrap();
        }
        let name = original["name"].as_str().unwrap();
        let case = Scenario {
            name: name.into(),
            arguments: vec!["1".into(), "18".into(), "3".into()],
            environment: vec![("CK_FAIL_OP".into(), "camera:271".into())],
        };
        let mut peer = PathPeer { prefixes: vec![] };
        // A native {} result lacks events and is reported as a mismatch. The
        // source lane still exactly exercises the original run-level contract.
        assert_eq!(
            check(
                &root.join("source"),
                &root.join("native"),
                &root.join("fixture"),
                &root.join("out"),
                None,
                None,
                &[case],
                &[("CK_AMBIENT".into(), "preserved".into())],
                28 * 1024u64.pow(3),
                &mut peer,
                &mut vec![]
            )
            .unwrap(),
            1
        );
        let folder = original["folder"].as_str().unwrap();
        assert!(!folder.chars().any(|c| "<>:\"/\\|?*".contains(c)));
        let receipt = root.join("out").join(folder).join("source/run.json");
        let text = fs::read_to_string(receipt)
            .unwrap()
            .replace(root.to_str().unwrap(), "$ROOT")
            .replace(&peer.prefixes[0], "$NAMESPACE");
        assert_eq!(text, original["receipt"]);
        let metadata: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(metadata["environment"]["CK_FAIL_OP"], "camera:271");
        let directories = fs::read_dir(root.join("out"))
            .unwrap()
            .filter_map(|e| {
                let e = e.unwrap();
                e.path().is_dir().then(|| e.file_name())
            })
            .collect::<Vec<_>>();
        assert_eq!(directories, vec![OsString::from(folder)]);
    }
}

#[cfg(unix)]
#[test]
fn actual_cli_raw_paths_stdin_environment_removal_and_namespace_cleanup() {
    use std::{
        io::Write,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
        process::Stdio,
    };
    let expected = fixture("stdin-env-original.json");
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let peer = root.join(OsString::from_vec(b"qemu-\xff".to_vec()));
    let output = root.join(OsString::from_vec(b"out-\xff".to_vec()));
    for (name, bytes) in [
        ("source", b"source".as_slice()),
        ("native", b"native"),
        ("fixture", b"fixture"),
    ] {
        fs::write(root.join(name), bytes).unwrap();
    }
    fs::write(&peer, expected["peer"].as_str().unwrap()).unwrap();
    fs::set_permissions(&peer, fs::Permissions::from_mode(0o755)).unwrap();
    let mut child =
        std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-camera-lifecycle"))
            .current_dir(root)
            .env("PATH", "/nonexistent")
            .env("CEREAL_FAKE", "1")
            .env("SPECTRA_ERROR_PROB", "0.4")
            .env("SPECTRA_ERROR_DT", "2")
            .env("SPECTRA_UNUSED", "preserved")
            .env("OPENPILOT_PREFIX", "must_be_replaced")
            .args([
                "--source",
                "./source",
                "--native",
                "native",
                "--fixture",
                "fixture",
                "--output",
            ])
            .arg(&output)
            .arg("--qemu")
            .arg(&peer)
            .args(["--sysroot", "/owned", "--case", "error-1-camera:271"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&b"token\n".repeat(2))
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(
        result.status.code(),
        Some(expected["exit"].as_i64().unwrap() as i32)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        expected["stdout"]
    );
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        expected["stderr"]
    );
    let mut prefixes = vec![];
    for lane in ["source", "native"] {
        let folder = output.join("error-1-camera%3A271").join(lane);
        assert_eq!(
            fs::read_to_string(folder.join("stdout")).unwrap(),
            expected["source_stdout"]
        );
        let receipt = openpilot_log_format::JsonValue::parse(
            &fs::read_to_string(folder.join("run.json")).unwrap(),
        )
        .unwrap();
        let environment = receipt.get("environment").unwrap();
        assert!(environment.get("CK_FAIL_OP").unwrap().text_eq("camera:271"));
        let prefix = environment
            .get("OPENPILOT_PREFIX")
            .unwrap()
            .to_utf8()
            .unwrap();
        assert_ne!(prefix, "must_be_replaced");
        assert!(!Path::new("/dev/shm")
            .join(format!("msgq_{prefix}"))
            .exists());
        prefixes.push(prefix);
    }
    assert_ne!(prefixes[0], prefixes[1]);
    let mut report: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("report.json")).unwrap()).unwrap();
    let free = report
        .as_object_mut()
        .unwrap()
        .remove("free_bytes")
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(free >= 1024u64.pow(3));
    assert_eq!(report, expected["report"]);
}
