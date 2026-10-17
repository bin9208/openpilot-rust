use openpilot_dev_tools::{
    jpeg_sanitizers_check,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
const FLAGS: &str = "-fsanitize=address,undefined -fno-omit-frame-pointer";
struct Fake {
    case: String,
    root: PathBuf,
    calls: Vec<Value>,
}
impl Runner for Fake {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert!(!spec.merge_output);
        let mut call = json!({"argv":spec.argv});
        if spec.argv[0] != "g++" {
            call["cwd"] = json!(spec.cwd);
            call["env"] = json!(spec.env);
            call["timeout"] = if spec.argv[0] == "cargo" {
                Value::Null
            } else {
                json!(spec.timeout.as_secs())
            };
        }
        self.calls.push(call);
        if self.case == "spawn-failed" {
            return Err("controlled spawn failure".into());
        }
        let mut result = Captured {
            code: 0,
            stdout: vec![],
            stderr: vec![],
        };
        if spec.argv[0] == "cargo" {
            result.code = if self.case == "build-failed" { 7 } else { 0 };
            result.stderr = b"build stderr\n".to_vec();
            let row =
                json!({"reason":"compiler-artifact","executable":self.root.join("ownership")});
            // Match the original oracle's byte stream, including JSON spacing.
            let line = format!(
                "{{\"reason\": \"compiler-artifact\", \"executable\": \"{}\"}}\n",
                row["executable"].as_str().unwrap()
            );
            result.stdout = match self.case.as_str() {
                "malformed-json" => b"{bad".to_vec(),
                "missing-executable" => vec![],
                "multiple-executables" => line.repeat(2).into_bytes(),
                "false-executable-record" => (line
                    + "{\"reason\": \"compiler-artifact\", \"executable\": []}\n")
                    .into_bytes(),
                "unicode-line-separator" => line.replace('\n', "\u{2028}").into_bytes(),
                "nonstring-executable" => {
                    b"{\"reason\": \"compiler-artifact\", \"executable\": true}\n".to_vec()
                }
                _ => line.into_bytes(),
            };
        } else if spec.argv[0] == "g++" {
            result.code = if self.case == "compiler-failed" { 9 } else { 0 };
            result.stdout = b" /controlled/libasan.so\n".to_vec();
        } else {
            result.code = if self.case == "run-failed" { 8 } else { 0 };
            result.stdout = b"run stdout\n".to_vec();
            result.stderr = b"run stderr\n".to_vec();
        }
        Ok(result)
    }
}
#[test]
fn original_fifteen_cases_preserve_commands_and_artifacts() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/jpeg-sanitizers/original.json")).unwrap();
    for original in oracle["cases"].as_array().unwrap() {
        let case = original["case"].as_str().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let target = root.join("target");
        let output = root.join("out");
        let rust = root.join("rust");
        fs::create_dir(&rust).unwrap();
        if case != "missing-cache" {
            for name in if case == "multiple-caches" {
                vec!["one", "two"]
            } else {
                vec!["one"]
            } {
                let path = target.join(format!(
                    "debug/build/openpilot-jpeg-{name}/out/jpeg/CMakeCache.txt"
                ));
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(
                    path,
                    format!(
                        "CMAKE_C_FLAGS:STRING={}\n",
                        if case == "wrong-flags" {
                            "wrong"
                        } else {
                            FLAGS
                        }
                    ),
                )
                .unwrap();
            }
        }
        let mut runner = Fake {
            case: case.into(),
            root: root.into(),
            calls: vec![],
        };
        let result = jpeg_sanitizers_check::check(
            &rust,
            &target,
            &output,
            &mut runner,
            26 * 1024u64.pow(3) - u64::from(case == "low-disk"),
        );
        assert_eq!(
            result.is_err(),
            !original["error"].is_null(),
            "{case}: {result:?}"
        );
        assert!(output.is_dir(), "output must exist even on low disk");
        let normalize = |v: &Value| -> Value {
            serde_json::from_str(
                &serde_json::to_string(v)
                    .unwrap()
                    .replace(rust.to_str().unwrap(), "$RUST")
                    .replace(root.to_str().unwrap(), "$ROOT"),
            )
            .unwrap()
        };
        // Python attempts a bool argv only after libasan lookup; process
        // construction then fails before spawn. CommandSpec represents argv strings.
        let expected_calls = if case == "nonstring-executable" {
            json!(&original["calls"].as_array().unwrap()[..2])
        } else {
            original["calls"].clone()
        };
        assert_eq!(normalize(&json!(runner.calls)), expected_calls, "{case}");
        let mut artifacts = serde_json::Map::new();
        for entry in fs::read_dir(&output).unwrap() {
            let path = entry.unwrap().path();
            artifacts.insert(
                path.file_name().unwrap().to_str().unwrap().into(),
                json!(fs::read_to_string(path).unwrap()),
            );
        }
        let actual = normalize(&Value::Object(artifacts));
        let expected = &original["artifacts"];
        assert_eq!(
            actual.as_object().unwrap().len(),
            expected.as_object().unwrap().len(),
            "{case}"
        );
        for (name, value) in expected.as_object().unwrap() {
            if name == "result.json" {
                assert_eq!(
                    serde_json::from_str::<Value>(actual[name].as_str().unwrap()).unwrap(),
                    serde_json::from_str::<Value>(value.as_str().unwrap()).unwrap()
                );
            } else {
                assert_eq!(&actual[name], value, "{case}/{name}");
            }
        }
    }
}
#[test]
fn real_cli_runs_fake_tools_and_rejects_build_or_ownership_failure() {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    for failure in ["", "build", "run"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let bin = root.join("bin");
        fs::create_dir(&bin).unwrap();
        let ownership = root.join("ownership");
        let scripts = [
            (bin.join("cargo"), "#!/bin/sh\nprintf '%s\\n' \"$CFLAGS\" > \"$JPEG_TEST_ROOT/flags\"\nprintf '{\"reason\":\"compiler-artifact\",\"executable\":\"%s/ownership\"}\\n' \"$JPEG_TEST_ROOT\"\nprintf 'build-stderr\\r\\n' >&2\n[ \"$JPEG_TEST_FAILURE\" != build ]\n"),
            (bin.join("g++"), "#!/bin/sh\nprintf '/controlled/absent/libasan.so\\n'\n"),
            (ownership.clone(), "#!/bin/sh\nprintf 'run-out\\r\\n'\nprintf 'run-err\\r\\n' >&2\n[ \"$JPEG_TEST_FAILURE\" != run ]\n"),
        ];
        for (path, script) in scripts {
            fs::write(&path, script).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let target = root.join("target");
        let cache = target.join("debug/build/openpilot-jpeg-one/out/jpeg/CMakeCache.txt");
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        fs::write(cache, format!("CMAKE_C_FLAGS:STRING={FLAGS}\n")).unwrap();
        let output = root.join("output");
        let result = Command::new(env!("CARGO_BIN_EXE_check-jpeg-sanitizers"))
            .args([
                "--target-dir",
                target.to_str().unwrap(),
                "--output",
                output.to_str().unwrap(),
            ])
            .env("PATH", &bin)
            .env("JPEG_TEST_ROOT", root)
            .env("JPEG_TEST_FAILURE", failure)
            .output()
            .unwrap();
        assert_eq!(result.status.success(), failure.is_empty(), "{:?}", result);
        assert_eq!(
            fs::read_to_string(root.join("flags")).unwrap(),
            format!("{FLAGS}\n")
        );
        assert!(output.join("build.jsonl").is_file());
        assert_eq!(
            fs::read_to_string(output.join("build.log")).unwrap(),
            "build-stderr\n"
        );
        assert_eq!(output.join("result.json").exists(), failure.is_empty());
        assert_eq!(output.join("run.log").exists(), failure != "build");
        if failure.is_empty() {
            assert_eq!(
                String::from_utf8(result.stdout).unwrap(),
                format!("{}\n", jpeg_sanitizers_check::SUCCESS)
            );
            // Loader diagnostics precede the script stderr and remain captured.
            let log = fs::read_to_string(output.join("run.log")).unwrap();
            assert!(log.starts_with("run-out\n"));
            assert!(log.ends_with("run-err\n"));
        }
    }
}
