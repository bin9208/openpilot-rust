use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_framework_check::check,
};
use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};
struct Peer<'a> {
    case: &'a Value,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let row = &self.case["calls"][self.calls];
        self.calls += 1;
        let actual = spec
            .effective_argv()
            .into_iter()
            .map(|v| v.into_string().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            row["command"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(spec.cwd, Some(PathBuf::from("@ROOT@")));
        assert!(spec.effective_env().is_empty());
        assert!(spec.inherit_stdin && spec.merge_output);
        assert_eq!(spec.timeout, std::time::Duration::MAX);
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        if row["name"] == self.case["fail"] && self.case["code"] == "spawn" {
            return Err("missing independent child".into());
        }
        Ok(Captured {
            code: if row["name"] == self.case["fail"] {
                self.case["code"].as_i64().unwrap() as i32
            } else {
                0
            },
            stdout: b"child-stdout\nchild-stderr\n".to_vec(),
            stderr: vec![],
        })
    }
}
fn normalize(text: &str, output: &Path) -> String {
    text.replace(output.to_str().unwrap(), "@OUTPUT@")
}
// Only the retired network/application executable prefixes change. Keep the independently
// captured argument tail, ordering, return codes, diagnostics and artifacts.
fn native_network_case(original: &Value, target: &str) -> Value {
    let mut case = original.clone();
    // Explicit policy migration: preserve the original below/equal/above
    // boundary, all child calls and every non-space artifact byte.
    if let Some(free) = case["space"].as_u64() {
        if ((26u64 << 30) - 1..=(26u64 << 30) + 1).contains(&free) {
            case["space"] = (free - (25u64 << 30)).into();
        }
    }
    if let Some(disk) = case["files"].get_mut("egl-build-disk.json") {
        let text = disk.as_str().unwrap();
        let mut parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["required_bytes"], 26u64 << 30);
        let free = parsed["available_bytes"].as_u64().unwrap();
        let changed = text
            .replace("27917287424", "1073741824")
            .replace("27917287425", "1073741825");
        parsed["required_bytes"] = (1u64 << 30).into();
        if ((26u64 << 30)..=(26u64 << 30) + 1).contains(&free) {
            parsed["available_bytes"] = (free - (25u64 << 30)).into();
        }
        assert_eq!(serde_json::from_str::<Value>(&changed).unwrap(), parsed);
        *disk = changed.into();
    }
    if let Some(calls) = case["calls"].as_array_mut() {
        for call in calls {
            let kind = call["name"]
                .as_str()
                .filter(|kind| matches!(*kind, "network" | "application"))
                .map(str::to_owned);
            if let Some(kind) = kind {
                let argv = call["command"].as_array_mut().unwrap();
                assert_eq!(argv[0], "@PYTHON@");
                assert_eq!(argv[1], format!("@ROOT@/rust/tools/check_ui_{kind}.py"));
                argv.splice(0..2, [Value::String(format!("{target}/check-ui-{kind}"))]);
            }
        }
    }
    for value in case["files"].as_object_mut().unwrap().values_mut() {
        let text = value.as_str().unwrap();
        *value = Value::String(
            text.replace(
                "\"@PYTHON@\", \"@ROOT@/rust/tools/check_ui_network.py\"",
                &format!("\"{target}/check-ui-network\""),
            )
            .replace(
                "\"@PYTHON@\",\n      \"@ROOT@/rust/tools/check_ui_network.py\"",
                &format!("\"{target}/check-ui-network\""),
            ),
        );
    }
    for value in case["files"].as_object_mut().unwrap().values_mut() {
        let text = value.as_str().unwrap();
        *value = Value::String(
            text.replace(
                "\"@PYTHON@\", \"@ROOT@/rust/tools/check_ui_application.py\"",
                &format!("\"{target}/check-ui-application\""),
            )
            .replace(
                "\"@PYTHON@\",\n      \"@ROOT@/rust/tools/check_ui_application.py\"",
                &format!("\"{target}/check-ui-application\""),
            ),
        );
    }
    case
}
#[test]
fn all_original_stages_calls_ledgers_logs_exit_and_failure_prefixes() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/ui-framework/original.json")).unwrap();
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 22);
    let legacy: Value =
        serde_json::from_str(include_str!("fixtures/ui-framework/legacy-original.json")).unwrap();
    assert_eq!(legacy["cases"].as_array().unwrap().len(), 7);
    let all_cases = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(legacy["cases"].as_array().unwrap().iter());
    for captured in all_cases {
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("output");
        if let Some(file) = match captured["name"].as_str().unwrap() {
            "log-directory" => Some("widgets.log"),
            "ledger-directory" => Some("invocations.json"),
            "disk-directory" => Some("egl-build-disk.json"),
            _ => None,
        } {
            fs::create_dir_all(output.join(file)).unwrap();
        }
        let mut printed = vec![];
        let mut disk_calls = 0;
        let mut case = native_network_case(captured, "relative target");
        for call in case["calls"].as_array_mut().unwrap() {
            for arg in call["command"].as_array_mut().unwrap() {
                *arg = Value::String(
                    arg.as_str()
                        .unwrap()
                        .replace("@OUTPUT@", output.to_str().unwrap()),
                );
            }
        }
        let mut peer = Peer {
            case: &case,
            calls: 0,
        };
        let actual = check(
            Path::new("@ROOT@"),
            Path::new("relative target"),
            &output,
            OsStr::new(":owned"),
            OsStr::new("@PYTHON@"),
            &mut peer,
            &mut |p| {
                assert_eq!(p, output);
                disk_calls += 1;
                Ok(case["space"].as_u64().unwrap())
            },
            &mut printed,
        );
        if let Some(exit) = case["exit"].as_i64() {
            assert_eq!(actual.unwrap(), exit as i32, "{}", case["name"]);
        } else {
            assert!(actual.is_err(), "{}", case["name"]);
        }
        assert_eq!(peer.calls, case["calls"].as_array().unwrap().len());
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            case["stdout"].as_str().unwrap()
        );
        assert_eq!(
            disk_calls,
            usize::from(
                case["calls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["name"] == "egl")
                    || case["name"] == "disk-below"
                    || case["name"] == "disk-directory"
            )
        );
        let actual = fs::read_dir(&output)
            .unwrap()
            .filter_map(|entry| {
                let p = entry.unwrap().path();
                if !p.is_file() {
                    return None;
                }
                Some((
                    p.file_name().unwrap().to_str().unwrap().to_owned(),
                    normalize(&fs::read_to_string(&p).unwrap(), &output),
                ))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let expected = case["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect();
        assert_eq!(actual, expected, "{} artifacts", case["name"]);
    }
}
#[test]
fn cli_argument_failures_create_no_output_and_preserve_exit_two() {
    for args in [
        vec![],
        vec!["--target", "t"],
        vec!["--target", "t", "--output", "o"],
        vec!["--bogus"],
        vec!["--target", "t", "--output", "o", "--display"],
    ] {
        let temp = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-ui-framework"))
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

#[test]
fn real_process_order_merged_diagnostics_ledgers_and_signal_exit_match_original() {
    use std::os::unix::fs::PermissionsExt;
    let captured: Value =
        serde_json::from_str(include_str!("fixtures/ui-framework/process-original.json")).unwrap();
    assert_eq!(captured["cases"].as_array().unwrap().len(), 5);
    assert_eq!(
        captured["source_sha256"],
        "657c638fc5cc2a071f45b8482ecb1866e3b6ee26eef4a04746a52226e5bdfcd3"
    );
    let interpreter = std::process::Command::new("python3")
        .args(["-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(interpreter.status.success());
    let interpreter = String::from_utf8(interpreter.stdout)
        .unwrap()
        .trim()
        .to_owned();
    for row in captured["cases"].as_array().unwrap() {
        let row = native_network_case(row, "@TARGET@");
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        let tools = root.join("rust/tools");
        fs::create_dir_all(&tools).unwrap();
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        let output = temp.path().join("output");
        for name in [
            "widgets",
            "scroll",
            "navigation",
            "stack",
            "scroller",
            "render",
            "forms",
            "network",
            "polygon",
            "diagnostics",
            "application",
            "fps",
            "egl",
        ] {
            let path = target.join(format!("check-ui-{name}"));
            fs::write(
                &path,
                format!(
                    "#!{interpreter}\n{}",
                    captured["child_source"].as_str().unwrap()
                ),
            )
            .unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let actual = std::process::Command::new(env!("CARGO_BIN_EXE_check-ui-framework"))
            .args([
                "--target",
                target.to_str().unwrap(),
                "--output",
                output.to_str().unwrap(),
                "--display",
                ":owned",
                "--python",
                &interpreter,
            ])
            .env("OPENPILOT_RUST_ROOT", &root)
            .env("FRAMEWORK_FAIL", row["fail"].as_str().unwrap_or(""))
            .env("FRAMEWORK_CODE", row["code"].as_i64().unwrap().to_string())
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), row["exit"].as_i64().map(|v| v as i32));
        assert_eq!(
            String::from_utf8(actual.stdout).unwrap(),
            row["stdout"].as_str().unwrap()
        );
        let observed = fs::read_dir(&output)
            .unwrap()
            .filter_map(|p| {
                let p = p.unwrap().path();
                let name = p.file_name().unwrap().to_str().unwrap().to_owned();
                if name == "egl-build-disk.json" {
                    return None;
                }
                let text = fs::read_to_string(p)
                    .unwrap()
                    .replace(root.to_str().unwrap(), "@ROOT@")
                    .replace(target.to_str().unwrap(), "@TARGET@")
                    .replace(output.to_str().unwrap(), "@OUTPUT@")
                    .replace(&interpreter, "@PYTHON@");
                Some((name, text))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let expected = row["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect();
        assert_eq!(observed, expected);
    }
}

#[test]
fn changed_reintroduced_original_fails_before_any_child() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("no child before source admission")
        }
    }
    let root = tempfile::tempdir().unwrap();
    let tools = root.path().join("rust/tools");
    fs::create_dir_all(&tools).unwrap();
    fs::write(tools.join("check_ui_framework.py"), b"changed original").unwrap();
    let output = root.path().join("out");
    let result = check(
        root.path(),
        Path::new("target"),
        &output,
        OsStr::new(":x"),
        OsStr::new("python"),
        &mut Never,
        &mut |_| panic!("no disk lookup before admission"),
        &mut vec![],
    );
    assert!(result.is_err());
    assert_eq!(fs::read_dir(output).unwrap().count(), 0);
}
