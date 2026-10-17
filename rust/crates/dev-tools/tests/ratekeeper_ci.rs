//! The shared Ratekeeper checker is a host tool, outside the Python helper.
use openpilot_dev_tools::ci_policy::parse;
use serde_json::Value;
use std::{fs, path::Path, process::Command};
const BUILD:&str="cargo build --manifest-path rust/Cargo.toml -p openpilot-dev-tools --bin check-ratekeeper --bin check-cutin-alert --bin check-selfdrive-state --bin check-alert-manager --bin check-selfdrive-events --bin check-alert-callbacks --bin check-selfdrive-helpers --locked";
const CHECK:&str="rust/target/debug/check-ratekeeper --binary \"$selfdrive_examples/ratekeeper\" --output \"$selfdrive_evidence/ratekeeper\"";
const GUARD:&str="\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048";
fn check(workflow: &Value) -> Result<(), String> {
    let job = &workflow["jobs"]["selfdrive-runtime"];
    if job["runs-on"] != "ubuntu-24.04"
        || job.get("if").is_some()
        || job.get("continue-on-error").is_some()
    {
        return Err("selfdrived host selection/required job changed".into());
    }
    let steps = job["steps"].as_array().ok_or("missing selfdrived steps")?;
    let mut builds = 0;
    let mut checks = 0;
    for step in steps {
        let Some(script) = step["run"].as_str() else {
            continue;
        };
        if script.contains("check_selfdrived_ratekeeper.py") {
            return Err("retired Python ratekeeper routed".into());
        }
        let lines = script.lines().map(str::trim).collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            if *line == BUILD {
                builds += 1;
                if index==0 || lines[index-1]!="\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 1024"{return Err("native checker build lacks its disk guard".into());}
            }
            if *line == CHECK {
                checks += 1;
                if index == 0 || lines[index - 1] != GUARD {
                    return Err("direct native ratekeeper call lacks original growth guard".into());
                }
                for declaration in [
                    "selfdrive_examples=\"$GITHUB_WORKSPACE/rust/target/debug/examples\"",
                    "selfdrive_evidence=\"$RUNNER_TEMP/selfdrived-native/reference\"",
                ] {
                    if !lines[..index].contains(&declaration) {
                        return Err(
                            "native ratekeeper bypasses original selected examples/evidence".into(),
                        );
                    }
                }
                if !script.contains("run_reference() {") || !script.contains("python \"$@\"") {
                    return Err("existing Python helper changed".into());
                }
            }
            if (*line == BUILD || *line == CHECK)
                && (step.get("if").is_some() || step.get("continue-on-error").is_some())
            {
                return Err("native ratekeeper build/check is optional".into());
            }
        }
    }
    if builds != 1 || checks != 1 {
        return Err("native ratekeeper build/check missing or duplicated".into());
    }
    Ok(())
}
fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
#[test]
fn native_ratekeeper_keeps_host_examples_and_hard_failing_selfdrive_route() {
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap();
    check(&workflow).unwrap();
}
#[test]
fn native_checker_nonzero_status_stops_the_actual_selected_command() {
    use std::os::unix::fs::PermissionsExt;
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap();
    check(&workflow).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let binary = temporary.path().join("rust/target/debug/check-ratekeeper");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary,"#!/bin/sh\n[ \"$1\" = --binary ] && [ \"$2\" = selected-examples/ratekeeper ] && [ \"$3\" = --output ] && [ \"$4\" = selected-evidence/ratekeeper ] || exit 89\nexit 7\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let script=format!("selfdrive_examples=selected-examples\nselfdrive_evidence=selected-evidence\n{CHECK}\ntouch wrongly-continued\n");
    let status = Command::new("/bin/bash")
        .args(["--noprofile", "--norc", "-e", "-c", &script])
        .current_dir(temporary.path())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(7));
    assert!(!temporary.path().join("wrongly-continued").exists());
}
#[test]
fn missing_softened_python_and_wrong_architecture_routes_cannot_pass() {
    let build=format!("\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 1024\n{BUILD}");
    let run=format!("selfdrive_examples=\"$GITHUB_WORKSPACE/rust/target/debug/examples\"\nselfdrive_evidence=\"$RUNNER_TEMP/selfdrived-native/reference\"\nrun_reference() {{\npython \"$@\"\n}}\n{GUARD}\n{CHECK}");
    let good = serde_json::json!({"jobs":{"selfdrive-runtime":{"runs-on":"ubuntu-24.04","steps":[{"run":build},{"run":run}]}}});
    check(&good).unwrap();
    for mutation in [
        "missing-build",
        "missing-check",
        "python",
        "softened",
        "conditional",
        "wrong-host",
        "wrong-examples",
        "no-guard",
    ] {
        let mut bad = good.clone();
        let job = &mut bad["jobs"]["selfdrive-runtime"];
        match mutation {
            "missing-build" => job["steps"][0]["run"] = "true".into(),
            "missing-check" => job["steps"][1]["run"] = "true".into(),
            "python" => {
                job["steps"][1]["run"] =
                    format!("{run}\nrun_reference rust/tools/check_selfdrived_ratekeeper.py").into()
            }
            "softened" => job["steps"][1]["continue-on-error"] = true.into(),
            "conditional" => job["steps"][1]["if"] = "always()".into(),
            "wrong-host" => job["runs-on"] = "ubuntu-24.04-arm".into(),
            "wrong-examples" => {
                job["steps"][1]["run"] = run
                    .replace("$selfdrive_examples/ratekeeper", "hardcoded/ratekeeper")
                    .into()
            }
            "no-guard" => job["steps"][1]["run"] = run.replace(GUARD, "").into(),
            _ => unreachable!(),
        }
        assert!(check(&bad).is_err(), "{mutation}");
    }
}
