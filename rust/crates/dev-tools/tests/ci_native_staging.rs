//! Actual workflow callers must build and explicitly select a host stager.
use openpilot_dev_tools::ci_policy::parse;
use serde_json::Value;
use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn direct_caller(line: &str) -> bool {
    line.contains("build_params_python.py ")
        || line.contains("check_encoder_ci.py ")
        || line.split_whitespace().next().is_some_and(|command| {
            command == "check-encoder-ci" || command.ends_with("/check-encoder-ci")
        })
}
fn callers(workflow: &Value) -> Result<usize, String> {
    let mut count = 0;
    for (name, job) in workflow["jobs"].as_object().ok_or("missing jobs")? {
        let Some(steps) = job["steps"].as_array() else {
            continue;
        };
        let mut ready = false;
        for step in steps {
            if step["uses"] == "./.github/actions/native-json11" {
                ready = true;
            }
            let script = step["run"].as_str().unwrap_or("");
            if script.contains("export PANDA_CI_BINARIES=") {
                if !ready
                    || !script.contains(
                        "export PANDA_CI_BINARIES=\"$GITHUB_WORKSPACE/rust/target/debug\"",
                    )
                    || !script.contains("--test panda_ci_retirement")
                {
                    return Err(format!(
                        "{name}: native Panda stager ownership not selected"
                    ));
                }
                count += 1;
            }
            for line in script.lines().filter(|line| direct_caller(line)) {
                if !ready || !line.contains("--json11-stager \"$JSON11_STAGER\"") {
                    return Err(format!("{name}: native stager not prepared/selected"));
                }
                count += 1;
            }
        }
    }
    Ok(count)
}
#[test]
fn all_runtime_and_template_callers_select_a_prepared_current_host_stager() {
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap();
    assert_eq!(callers(&workflow).unwrap(), 23);
    let panda =
        fs::read_to_string(root().join("rust/crates/dev-tools/src/panda_ci_check.rs")).unwrap();
    assert!(panda.contains("fn executables(bin: &Path)"));
    assert!(panda.contains("let binary_sha256 = executables(&options.binaries)"));
    assert!(panda.contains("&self.bin(\"stage-json11\")"));
    assert!(panda.contains("\"json11_stage.rs\""));
    assert!(panda.contains("\"bin/stage_json11.rs\""));
    let template =
        parse(&fs::read_to_string(root().join("rust/crates/carrot-man/ci-jobs.yml")).unwrap())
            .unwrap();
    assert_eq!(callers(&serde_json::json!({"jobs":template})).unwrap(), 1);
    for (job, value) in workflow["jobs"].as_object().unwrap() {
        let Some(steps) = value["steps"].as_array() else {
            continue;
        };
        for (index, step) in steps.iter().enumerate() {
            if step["uses"] == "./.github/actions/native-json11" {
                let mut changed = workflow.clone();
                changed["jobs"][job]["steps"]
                    .as_array_mut()
                    .unwrap()
                    .remove(index);
                assert!(callers(&changed).is_err(), "{job}");
            }
            if step["run"]
                .as_str()
                .is_some_and(|run| run.lines().any(direct_caller))
            {
                let mut changed = workflow.clone();
                changed["jobs"][job]["steps"][index]["run"] = Value::String(
                    step["run"]
                        .as_str()
                        .unwrap()
                        .replace("--json11-stager", "--missing-stager"),
                );
                assert!(callers(&changed).is_err(), "{job}");
            }
            if step["run"]
                .as_str()
                .is_some_and(|run| run.contains("export PANDA_CI_BINARIES="))
            {
                let mut changed = workflow.clone();
                changed["jobs"][job]["steps"][index]["run"] = Value::String(
                    step["run"]
                        .as_str()
                        .unwrap()
                        .replace("rust/target/debug\"", "wrong-target\""),
                );
                assert!(callers(&changed).is_err(), "{job}: Panda binary selection");
            }
        }
    }
}
#[test]
fn host_stager_action_reserves_before_toolchain_work_and_separates_cross_flags() {
    let action = parse(
        &fs::read_to_string(root().join(".github/actions/native-json11/action.yml")).unwrap(),
    )
    .unwrap();
    assert_eq!(action["runs"]["using"], "composite");
    let script = action["runs"]["steps"][0]["run"].as_str().unwrap();
    let guard = script
        .find("\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\"")
        .unwrap();
    assert!(guard < script.find("rustc +1.94.0").unwrap());
    assert!(guard < script.find("cargo +1.94.0 build").unwrap());
    for token in [
        "--locked --features ci-receipt-metadata-native --bin stage-json11 --bin ci-receipt-metadata --target \"$native_host\"",
        "-u CARGO_BUILD_TARGET",
        "-u RUSTFLAGS",
        "-u CARGO_ENCODED_RUSTFLAGS",
        "CARGO_INCREMENTAL=0",
        "CARGO_BUILD_JOBS=2",
        "JSON11_STAGER=%s/rust/target/%s/debug/stage-json11",
        "CI_RECEIPT_METADATA=%s/rust/target/%s/debug/ci-receipt-metadata",
        "NATIVE_TOOLS_HOST=%s",
    ] {
        assert!(script.contains(token), "{token}");
    }
}

#[test]
fn action_executes_guard_first_and_exports_the_built_host_binary() {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    let action = parse(
        &fs::read_to_string(root().join(".github/actions/native-json11/action.yml")).unwrap(),
    )
    .unwrap();
    let script = action["runs"]["steps"][0]["run"].as_str().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let tools = temp.path().join("tools");
    fs::create_dir_all(workspace.join("rust/tools")).unwrap();
    fs::create_dir(&tools).unwrap();
    let guard = workspace.join("rust/tools/check-disk-space");
    let rustc = tools.join("rustc");
    let cargo = tools.join("cargo");
    fs::write(
        &guard,
        "#!/bin/bash\nprintf 'guard\\n' >> \"$TRACE\"\nexit \"${GUARD_STATUS:-0}\"\n",
    )
    .unwrap();
    fs::write(&rustc, "#!/bin/bash\nprintf 'rustc\\n' >> \"$TRACE\"\ntest \"$1\" = +1.94.0\nprintf 'host: aarch64-unknown-linux-gnu\\n'\n").unwrap();
    fs::write(
        &cargo,
        r#"#!/bin/bash
set -eu
printf 'cargo\n' >> "$TRACE"
test "$1" = +1.94.0
test -z "${RUSTFLAGS+x}${CARGO_ENCODED_RUSTFLAGS+x}${CARGO_BUILD_TARGET+x}${RUSTC_WRAPPER+x}${RUSTC_WORKSPACE_WRAPPER+x}${CC+x}${CXX+x}${CFLAGS+x}${CXXFLAGS+x}${CPPFLAGS+x}${LDFLAGS+x}"
test "$CARGO_INCREMENTAL" = 0
test "$CARGO_BUILD_JOBS" = 2
target= bins=()
while (($#)); do
  case "$1" in --target) target=$2; shift ;; --bin) bins+=("$2"); shift ;; esac
  shift
done
test "$target" = aarch64-unknown-linux-gnu
test "${bins[*]}" = 'stage-json11 ci-receipt-metadata'
if [ "${CARGO_STATUS:-0}" != 0 ]; then exit "$CARGO_STATUS"; fi
mkdir -p "$CARGO_TARGET_DIR/$target/debug"
for binary in "${bins[@]}"; do
  printf '#!/bin/bash\nprintf "%%s\\n" "%s"\n' "$binary" > "$CARGO_TARGET_DIR/$target/debug/$binary"
  chmod +x "$CARGO_TARGET_DIR/$target/debug/$binary"
done
"#,
    )
    .unwrap();
    for file in [&guard, &rustc, &cargo] {
        fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let trace = temp.path().join("trace");
    let export = temp.path().join("environment");
    let run = |guard_status: &str, cargo_status: &str| {
        Command::new("/bin/bash")
            .args(["-c", script])
            .env("PATH", format!("{}:/usr/bin:/bin", tools.display()))
            .env("GITHUB_WORKSPACE", &workspace)
            .env("GITHUB_ENV", &export)
            .env("TRACE", &trace)
            .env("GUARD_STATUS", guard_status)
            .env("CARGO_STATUS", cargo_status)
            .env("RUSTFLAGS", "invalid-cross-flags")
            .env("CARGO_BUILD_TARGET", "invalid-cross-target")
            .env("CC", "invalid-cross-compiler")
            .env("CFLAGS", "invalid-cross-flags")
            .env("CARGO_ENCODED_RUSTFLAGS", "invalid-cross-flags")
            .env("RUSTC_WRAPPER", "invalid-cross-wrapper")
            .env("RUSTC_WORKSPACE_WRAPPER", "invalid-cross-wrapper")
            .env("CXX", "invalid-cross-compiler")
            .env("CXXFLAGS", "invalid-cross-flags")
            .env("CPPFLAGS", "invalid-cross-flags")
            .env("LDFLAGS", "invalid-cross-flags")
            .output()
            .unwrap()
    };
    let output = run("0", "0");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&trace).unwrap(), "guard\nrustc\ncargo\n");
    let binary = workspace.join("rust/target/aarch64-unknown-linux-gnu/debug/stage-json11");
    let metadata =
        workspace.join("rust/target/aarch64-unknown-linux-gnu/debug/ci-receipt-metadata");
    for (path, expected) in [
        (&binary, "stage-json11\n"),
        (&metadata, "ci-receipt-metadata\n"),
    ] {
        assert!(path.is_file());
        let output = Command::new(path).output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
    assert_eq!(
        fs::read_to_string(&export).unwrap(),
        format!(
            "JSON11_STAGER={}\nCI_RECEIPT_METADATA={}\nNATIVE_TOOLS_HOST=aarch64-unknown-linux-gnu\n",
            binary.display(), metadata.display()
        )
    );
    for (guard_status, cargo_status, code, expected) in [
        ("42", "0", 42, "guard\n"),
        ("0", "43", 43, "guard\nrustc\ncargo\n"),
    ] {
        fs::remove_file(&trace).unwrap();
        fs::remove_file(&export).ok();
        assert_eq!(run(guard_status, cargo_status).status.code(), Some(code));
        assert_eq!(fs::read_to_string(&trace).unwrap(), expected);
        assert!(!export.exists());
    }
}
