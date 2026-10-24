//! Unchanged original orchestration captured before native implementation.
use openpilot_dev_tools::ci_policy::{validate_local_routing, TRUSTED_SOURCE};
use openpilot_dev_tools::{
    camera_ci_check::{execute, Options},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use std::{fs, path::Path, time::Duration};
fn corpus() -> Value {
    let mut original: Value =
        serde_json::from_str(include_str!("fixtures/camera-ci-check/original.json")).unwrap();
    let constants: Value = serde_json::from_str(include_str!(
        "fixtures/camera-ci-check/constants-original.json"
    ))
    .unwrap();
    original["cases"]
        .as_array_mut()
        .unwrap()
        .extend(constants["cases"].as_array().unwrap().iter().cloned());
    original
}
fn substitute(text: &str, root: &Path) -> String {
    text.replace("$ROOT", root.to_str().unwrap())
        .replace("$PYTHON", "/independent/python")
}
fn native(text: &str) -> String {
    text.replace(
        "\"$PYTHON\",\n    \"$ROOT/root/rust/tools/camerad_runtime_check.py\"",
        "\"$ROOT/bin/check-camerad-runtime\"",
    )
}
struct Peer<'a> {
    row: &'a Value,
    root: &'a Path,
    at: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let expected = &self.row["calls"][self.at];
        self.at += 1;
        let mut argv = expected["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| substitute(v.as_str().unwrap(), self.root))
            .collect::<Vec<_>>();
        if argv[0] == "/independent/python" {
            argv.drain(..2);
            argv.insert(
                0,
                self.root
                    .join("bin/check-camerad-runtime")
                    .display()
                    .to_string(),
            );
        }
        assert_eq!(
            spec.effective_argv(),
            argv.iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(spec.cwd, Some(self.root.join("root")));
        assert_eq!(spec.timeout, Duration::MAX);
        assert!(spec.inherit_stdin);
        assert!(spec.input.is_none());
        assert_eq!(
            (spec.stdout_limit, spec.stderr_limit),
            (usize::MAX, usize::MAX)
        );
        assert!(!spec.inherit_output && !spec.merge_output);
        let name = expected["name"].as_str().unwrap();
        let code = self.row["failure"]
            .as_str()
            .filter(|v| *v == name)
            .map_or(0, |_| 7);
        Ok(Captured {
            code,
            stdout: self.row["files"][format!("commands/{name}.stdout")]
                .as_str()
                .unwrap()
                .as_bytes()
                .to_vec(),
            stderr: self.row["files"][format!("commands/{name}.stderr")]
                .as_str()
                .unwrap()
                .as_bytes()
                .to_vec(),
        })
    }
}
fn files(root: &Path, path: &Path, out: &mut std::collections::BTreeMap<String, String>) {
    for entry in fs::read_dir(path).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            files(root, &p, out);
        } else {
            out.insert(
                p.strip_prefix(root).unwrap().display().to_string(),
                fs::read_to_string(p).unwrap(),
            );
        }
    }
}
#[test]
fn original_ordered_commands_growth_and_all_partial_artifacts_are_retained() {
    for row in corpus()["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("out")).unwrap();
        fs::create_dir(root.join("bin")).unwrap();
        for name in [
            "third_party/linux/include/msm_camsensor_sdk.h",
            "openpilot/cereal/include/c++.capnp",
            "opendbc_repo/opendbc/car/car.capnp",
            "openpilot/cereal/custom.capnp",
            "openpilot/cereal/deprecated.capnp",
            "openpilot/cereal/log.capnp",
        ] {
            let p = root.join("root").join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, format!("synthetic {name}\n")).unwrap();
        }
        let opt = Options {
            root: root.join("root"),
            output: root.join("out"),
            binaries: root.join("bin"),
            compiler: "clang++-18".into(),
        };
        let mut peer = Peer { row, root, at: 0 };
        let mut guards = vec![];
        let mut stdout = vec![];
        let mut stderr = vec![];
        let result = execute(
            &opt,
            &mut peer,
            &mut |path, growth| {
                guards.push(serde_json::json!([path, growth]));
                if row["failure"] == "disk" {
                    Err("controlled low disk".into())
                } else {
                    Ok(())
                }
            },
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "failure={}",
            row["failure"]
        );
        assert_eq!(peer.at, row["calls"].as_array().unwrap().len());
        assert_eq!(
            serde_json::to_string(&guards)
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
            serde_json::to_string(&row["spaces"]).unwrap()
        );
        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            row["stdout"].as_str().unwrap()
        );
        assert_eq!(
            String::from_utf8(stderr).unwrap(),
            row["stderr"].as_str().unwrap()
        );
        let mut actual = std::collections::BTreeMap::new();
        files(&root.join("out"), &root.join("out"), &mut actual);
        assert_eq!(actual.len(), row["files"].as_object().unwrap().len());
        for (name, text) in actual {
            let expected = native(row["files"][&name].as_str().unwrap());
            let actual = text.replace(root.to_str().unwrap(), "$ROOT");
            assert_eq!(actual, expected, "{}/{}", row["failure"], name);
        }
    }
}

#[test]
fn original_main_preflight_hashes_and_failure_order_are_preserved() {
    use openpilot_dev_tools::camera_ci_check::prepare;
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/camera-ci-check/main-original.json")).unwrap();
    for row in corpus["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("root")).unwrap();
        fs::create_dir(root.join("bin")).unwrap();
        for name in row["sources"].as_array().unwrap() {
            let name = name.as_str().unwrap();
            let path = root.join("root").join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("source {name}\n")).unwrap();
        }
        for name in row["binaries"].as_array().unwrap() {
            let name = name.as_str().unwrap();
            let path = root.join("bin").join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("binary {name}\n")).unwrap();
        }
        fs::write(
            root.join("bin/check-camerad-runtime"),
            "binary check-camerad-runtime\n",
        )
        .unwrap();
        if row["mode"] == "missing-binary" {
            fs::remove_file(root.join("bin/examples/sensor_policy")).unwrap();
        }
        if row["mode"] == "existing-output" {
            fs::create_dir(root.join("out")).unwrap();
        }
        let options = Options {
            root: root.join("root"),
            output: root.join("out"),
            binaries: root.join("bin"),
            compiler: "owned-clang".into(),
        };
        let argv = [
            "check-camerad-ci".into(),
            "--binaries".into(),
            root.join("bin").into_os_string(),
            "--output".into(),
            root.join("out").into_os_string(),
            "--compiler".into(),
            "owned-clang".into(),
        ];
        let mut guards = vec![];
        let result = prepare(&options, &argv, &mut |path, growth| {
            guards.push(serde_json::json!([path, growth]));
            if row["mode"] == "disk" {
                Err("controlled disk refusal".into())
            } else {
                Ok(())
            }
        });
        assert_eq!(result.is_ok(), row["error"].is_null(), "{}", row["mode"]);
        assert_eq!(
            root.join("out").exists(),
            row["output_exists"].as_bool().unwrap()
        );
        assert_eq!(
            serde_json::to_string(&guards)
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
            row["spaces"].to_string()
        );
        if result.is_ok() {
            let mut expected: Value =
                serde_json::from_str(row["files"]["invocation.json"].as_str().unwrap()).unwrap();
            expected["argv"][0] = "check-camerad-ci".into();
            use sha2::{Digest, Sha256};
            expected["binary_sha256"]["$ROOT/bin/check-camerad-runtime"] =
                format!("{:x}", Sha256::digest(b"binary check-camerad-runtime\n")).into();
            let actual: Value = serde_json::from_str(
                &fs::read_to_string(root.join("out/invocation.json"))
                    .unwrap()
                    .replace(root.to_str().unwrap(), "$ROOT"),
            )
            .unwrap();
            assert_eq!(actual, expected, "{}", row["mode"]);
        } else if root.join("out").exists() {
            assert_eq!(fs::read_dir(root.join("out")).unwrap().count(), 0);
        }
    }
}

#[test]
fn actual_cli_retains_original_admission_and_preflight_side_effects_without_python() {
    let captures: Value =
        serde_json::from_str(include_str!("fixtures/camera-ci-check/cli-original.json")).unwrap();
    for case in captures["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let out = root.join("out");
        if case["existing"].as_bool().unwrap() {
            fs::create_dir(&out).unwrap();
        }
        let args = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| substitute(v.as_str().unwrap(), root))
            .collect::<Vec<_>>();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-ci"))
            .args(args)
            .current_dir(root)
            .env("PATH", "/nonexistent")
            .env(
                "OPENPILOT_RUST_ROOT",
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            )
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            case["exit"].as_i64().map(|v| v as i32),
            "{}",
            case["name"]
        );
        assert_eq!(
            !result.stdout.is_empty(),
            case["stdout"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            !result.stderr.is_empty(),
            case["stderr"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            out.exists(),
            case["output_exists"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        let mut files = if out.exists() {
            fs::read_dir(&out)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        files.sort();
        assert_eq!(serde_json::to_value(files).unwrap(), case["files"]);
        assert!(!root.join("unused").exists());
    }
}

#[test]
fn real_missing_compiler_retains_only_original_prelaunch_artifacts() {
    let capture: Value = serde_json::from_str(include_str!(
        "fixtures/camera-ci-check/launch-original.json"
    ))
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let binaries = root.join("bin");
    fs::create_dir(&binaries).unwrap();
    for name in capture["binaries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .chain(["check-camerad-runtime"])
    {
        let path = binaries.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("binary {name}\n")).unwrap();
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let output = root.join("out");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-ci"))
        .args([
            "--binaries",
            binaries.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .current_dir(root)
        .env("PATH", "/nonexistent")
        .env("OPENPILOT_RUST_ROOT", &repo)
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        capture["exit"].as_i64().map(|v| v as i32)
    );
    assert_eq!(
        !result.stdout.is_empty(),
        capture["stdout"].as_bool().unwrap()
    );
    assert_eq!(
        !result.stderr.is_empty(),
        capture["stderr"].as_bool().unwrap()
    );
    let mut actual = std::collections::BTreeMap::new();
    files(&output, &output, &mut actual);
    assert!(actual.remove("invocation.json").is_some());
    assert_eq!(actual.len(), capture["files"].as_object().unwrap().len());
    for (name, text) in actual {
        assert_eq!(
            text.replace(root.to_str().unwrap(), "$ROOT")
                .replace(repo.to_str().unwrap(), "$REPO"),
            capture["files"][&name].as_str().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn mandatory_camera_gate_builds_native_controller_and_orchestrator_before_live_checks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow = openpilot_dev_tools::ci_policy::parse(
        &fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap(),
    )
    .unwrap();
    let steps = workflow["jobs"]["camera-runtime"]["steps"]
        .as_array()
        .unwrap();
    let build = steps
        .iter()
        .position(|s| s["name"] == "Build and validate native camera source generators")
        .unwrap();
    let live = steps
        .iter()
        .position(|s| {
            s["name"] == "Require original camera comparisons and continuous driver fixtures"
        })
        .unwrap();
    assert!(build < live);
    for step in [&steps[build], &steps[live]] {
        assert_eq!(step["if"], TRUSTED_SOURCE);
        assert!(step.get("continue-on-error").is_none());
    }
    let build_command = steps[build]["run"].as_str().unwrap();
    for required in [
        "--features camera-runtime-native",
        "--test camera_ci_check",
        "--test camera_runtime_check",
        "--test camera_runtime_controller",
        "--test camera_runtime_native",
        "--bin check-camerad-ci",
        "--bin check-camerad-runtime",
        "--bin camera-runtime-test-peer",
        "--locked",
        "-D warnings",
    ] {
        assert!(build_command.contains(required), "missing {required}");
    }
    let live_command = steps[live]["run"].as_str().unwrap();
    assert!(
        live_command.contains("rust/target/debug/check-camerad-ci --binaries rust/target/debug")
    );
    assert!(!live_command.contains("python"));
}

fn camera_native_peer_policy(job: &Value) -> std::result::Result<(), String> {
    validate_local_routing(&serde_json::json!({"jobs":{"camera-runtime":job}}))?;
    if job.get("if").is_some() || job.get("continue-on-error").is_some() {
        return Err("camera job must remain mandatory".into());
    }
    let check_environment = |environment: &Value| -> std::result::Result<(), String> {
        if let Some(environment) = environment.as_object() {
            for (key, value) in environment {
                if key.to_ascii_lowercase().contains("python")
                    || value.to_string().contains("camera-native/python")
                {
                    return Err(format!("camera retains Python environment: {key}"));
                }
            }
        }
        Ok(())
    };
    check_environment(&job["env"])?;
    let steps = job["steps"].as_array().ok_or("camera steps missing")?;
    let mut commands = vec![];
    for step in steps {
        if step["uses"]
            .as_str()
            .is_some_and(|value| value.starts_with("actions/setup-python@"))
        {
            return Err("camera retains Python setup".into());
        }
        check_environment(&step["env"])?;
        if let Some(run) = step["run"].as_str() {
            let lower = run.to_ascii_lowercase();
            for forbidden in [
                "python",
                "pip ",
                "pip3",
                "pycapnp",
                "pyzmq",
                "cython",
                "setuptools",
                "pytest",
                "camera-native/python",
                "imports.txt",
            ] {
                if lower.contains(forbidden) {
                    return Err(format!(
                        "camera retains Python command/dependency: {forbidden}"
                    ));
                }
            }
            if (step["if"] != TRUSTED_SOURCE
                && !matches!(
                    step["name"].as_str().unwrap_or(""),
                    "Deny fork source and require GitHub-hosted Linux X64"
                        | "Seal private source and index after maintained checks"
                        | "Clean only uploaded owned source alias and target"
                ))
                || step.get("continue-on-error").is_some()
            {
                return Err("camera native commands must remain unconditional".into());
            }
            commands.extend(run.lines().map(str::trim));
        }
    }
    for required in [
        "cargo test --manifest-path rust/Cargo.toml -p openpilot-camerad --all-targets --locked",
        "cargo test --manifest-path rust/Cargo.toml -p openpilot-camera-kernel -p openpilot-camerad-runtime --features native-skip-miri --all-targets --locked",
        "cargo test --manifest-path rust/Cargo.toml -p openpilot-msgq --features native-skip-miri --test vision --test vision_server --test vision_native --locked",
        "cargo build --manifest-path rust/Cargo.toml -p openpilot-camerad -p openpilot-camera-kernel -p openpilot-camerad-runtime --features native-skip-miri --bins --examples --locked",
        "cargo clippy --manifest-path rust/Cargo.toml -p openpilot-camerad -p openpilot-camera-kernel -p openpilot-camerad-runtime --features native-skip-miri --all-targets --locked -- -D warnings",
        "rust/target/debug/check-camerad-ci --binaries rust/target/debug --output \"$RUNNER_TEMP/camera-native/oracles\"",
    ] {
        if commands.iter().filter(|command| **command == required).count() != 1 {
            return Err(format!("camera must run native command exactly once: {required}"));
        }
    }
    if !commands.iter().any(|command| {
        command.contains(
            "apt-get -o \"APT::Architectures=$(dpkg --print-architecture)\" install -y capnproto libcapnp-dev g++ clang-18 libclang-rt-18-dev libzmq3-dev",
        )
    }) {
        return Err("camera native schema/compiler/IPC prerequisites missing".into());
    }
    if !commands.iter().any(|command| {
        command.contains(
            "rustup toolchain install 1.94.0 --profile minimal --component rustfmt,clippy",
        )
    }) {
        return Err("camera pinned Rust toolchain missing".into());
    }
    Ok(())
}

fn camera_job() -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow = openpilot_dev_tools::ci_policy::parse(
        &fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap(),
    )
    .unwrap();
    workflow["jobs"]["camera-runtime"].clone()
}

#[test]
fn camera_gate_uses_native_visionipc_peers_without_python_setup_or_bindings() {
    camera_native_peer_policy(&camera_job()).unwrap();
}

#[test]
fn camera_native_peer_policy_rejects_python_and_lost_or_skipped_native_coverage() {
    let baseline = camera_job();
    camera_native_peer_policy(&baseline).unwrap();
    for mut step in [
        serde_json::json!({"uses":"actions/setup-python@v6"}),
        serde_json::json!({"run":"python -m pip install pycapnp Cython"}),
        serde_json::json!({"run":"python rust/tools/build_visionipc_python.py --output binding"}),
        serde_json::json!({"run":"python -c 'import msgq.visionipc'"}),
        serde_json::json!({"run":"pip3 install pyzmq"}),
        serde_json::json!({"run":"true","env":{"PYTHONPATH":"binding"}}),
    ] {
        step["if"] = TRUSTED_SOURCE.into();
        if step.get("run").is_some() {
            step["working-directory"] = "${{ env.CI296_SOURCE }}".into();
        }
        let mut changed = baseline.clone();
        changed["steps"].as_array_mut().unwrap().push(step);
        assert!(camera_native_peer_policy(&changed).is_err());
    }
    let mut changed = baseline.clone();
    changed["env"]["PYTHONPATH"] = "binding".into();
    assert!(camera_native_peer_policy(&changed).is_err());
    for target in ["vision", "vision_server", "vision_native"] {
        let mut changed = baseline.clone();
        let step = changed["steps"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|step| {
                step["run"]
                    .as_str()
                    .is_some_and(|run| run.contains("-p openpilot-msgq"))
            })
            .unwrap();
        step["run"] = step["run"]
            .as_str()
            .unwrap()
            .replace(&format!(" --test {target}"), "")
            .into();
        assert!(
            camera_native_peer_policy(&changed).is_err(),
            "missing {target}"
        );
    }
    for condition in ["if", "continue-on-error"] {
        let mut changed = baseline.clone();
        let step = changed["steps"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|step| {
                step["run"]
                    .as_str()
                    .is_some_and(|run| run.contains("-p openpilot-msgq"))
            })
            .unwrap();
        step[condition] = true.into();
        assert!(camera_native_peer_policy(&changed).is_err(), "{condition}");
    }
}
