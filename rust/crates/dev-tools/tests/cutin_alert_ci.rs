//! Preserve the surrounding source comparisons while retiring the fixed cut-in checker.
use openpilot_dev_tools::ci_policy::parse;
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

const BUILD: &str = "cargo build --manifest-path rust/Cargo.toml -p openpilot-dev-tools --bin check-ratekeeper --bin check-cutin-alert --locked";
const CHECK: &str = "rust/target/debug/check-cutin-alert --binary \"$selfdrive_examples/cutin_alerts\" --output \"$selfdrive_evidence/cutin\"";
const GUARD: &str = "\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048";
const BUILD_GUARD: &str = "\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 1024";
const EXAMPLES: &str = "selfdrive_examples=\"$GITHUB_WORKSPACE/rust/target/debug/examples\"";
const EVIDENCE: &str = "selfdrive_evidence=\"$RUNNER_TEMP/selfdrived-native/reference\"";
const REFERENCES: [&str; 9] = [
    "run_reference rust/tools/generate_selfdrive_alerts.py --output rust/crates/selfdrived/data/alerts.json --check",
    "run_reference rust/tools/check_selfdrive_state.py --binary \"$selfdrive_examples/selfdrive_state\" --output \"$selfdrive_evidence/state\"",
    "run_reference rust/tools/check_alert_manager.py --binary \"$selfdrive_examples/alert_manager\" --output \"$selfdrive_evidence/alert-manager\"",
    "run_reference rust/tools/check_selfdrive_events.py --binary \"$selfdrive_examples/events_trace\" --output \"$selfdrive_evidence/events\"",
    "run_reference rust/tools/check_alert_callbacks.py --binary \"$selfdrive_examples/alert_callbacks\" --output \"$selfdrive_evidence/callbacks\"",
    "run_reference rust/tools/check_selfdrive_params.py --native \"$selfdrive_examples/alert_params\" --binding \"$SELFDRIVE_PARAMS_BINDING\" --output \"$selfdrive_evidence/params\"",
    "run_reference rust/tools/check_selfdrive_helpers.py --native \"$selfdrive_examples/selfdrive_helpers\" --output \"$selfdrive_evidence/helpers\"",
    "run_reference rust/tools/check_car_specific.py --binary \"$selfdrive_examples/car_specific\" --binding \"$SELFDRIVE_PARAMS_BINDING\" --output \"$selfdrive_evidence/car-specific\"",
    "run_reference rust/tools/check_selfdrived_controller.py --binary \"$selfdrive_examples/controller\" --binding \"$SELFDRIVE_PARAMS_BINDING\" --output \"$selfdrive_evidence/controller\"",
];

fn check(workflow: &Value) -> Result<(), String> {
    let job = &workflow["jobs"]["selfdrive-runtime"];
    if job["runs-on"] != "ubuntu-24.04"
        || job.get("if").is_some()
        || job.get("continue-on-error").is_some()
    {
        return Err("selfdrived host or required job changed".into());
    }
    let steps = job["steps"].as_array().ok_or("missing selfdrived steps")?;
    let mut builds = 0;
    let mut checks = 0;
    for step in steps {
        let Some(script) = step["run"].as_str() else {
            continue;
        };
        if script.contains("check_cutin_alerts.py") {
            return Err("retired Python cut-in checker routed".into());
        }
        let lines = script.lines().map(str::trim).collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            if *line == BUILD {
                builds += 1;
                if index == 0 || lines[index - 1] != BUILD_GUARD {
                    return Err("native checker build lacks its 1024 MiB guard".into());
                }
            }
            if *line == CHECK {
                checks += 1;
                if index < 2
                    || lines[index - 1] != GUARD
                    || lines[index - 2] != REFERENCES[7]
                    || lines.get(index + 1) != Some(&REFERENCES[8])
                {
                    return Err("native cut-in check moved or lost its 2048 MiB guard".into());
                }
                for declaration in [EXAMPLES, EVIDENCE] {
                    if !lines[..index].contains(&declaration) {
                        return Err("native cut-in bypasses selected examples/evidence".into());
                    }
                }
                let helper = ["run_reference() {", GUARD, "python \"$@\"", "}"];
                if !lines.windows(helper.len()).any(|part| part == helper) {
                    return Err("original run_reference helper changed".into());
                }
                for command in REFERENCES {
                    if lines.iter().filter(|line| **line == command).count() != 1 {
                        return Err(format!(
                            "existing comparison dropped or duplicated: {command}"
                        ));
                    }
                }
            }
            if (*line == BUILD || *line == CHECK)
                && (step.get("if").is_some() || step.get("continue-on-error").is_some())
            {
                return Err("native cut-in build/check is optional".into());
            }
        }
    }
    if builds != 1 || checks != 1 {
        return Err("native cut-in build/check missing or duplicated".into());
    }
    Ok(())
}

fn workflow() -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    parse(&fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap()).unwrap()
}

#[test]
fn native_cutin_preserves_host_source_comparisons_and_direct_growth_guard() {
    check(&workflow()).unwrap();
}

#[test]
fn actual_selected_native_command_failure_stops_the_shell() {
    use std::os::unix::fs::PermissionsExt;
    let workflow = workflow();
    check(&workflow).unwrap();
    let command = workflow["jobs"]["selfdrive-runtime"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|step| step["run"].as_str())
        .flat_map(str::lines)
        .map(str::trim)
        .find(|line| *line == CHECK)
        .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let binary = temporary.path().join("rust/target/debug/check-cutin-alert");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, "#!/bin/sh\n[ \"$1\" = --binary ] && [ \"$2\" = selected-examples/cutin_alerts ] && [ \"$3\" = --output ] && [ \"$4\" = selected-evidence/cutin ] || exit 89\nexit 7\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let script = format!("selfdrive_examples=selected-examples\nselfdrive_evidence=selected-evidence\n{command}\ntouch wrongly-continued\n");
    let status = Command::new("/bin/bash")
        .args(["--noprofile", "--norc", "-eo", "pipefail", "-c", &script])
        .current_dir(temporary.path())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(7));
    assert!(!temporary.path().join("wrongly-continued").exists());
}

#[test]
fn missing_optional_softened_python_or_changed_source_routes_are_rejected() {
    let build = format!("{BUILD_GUARD}\n{BUILD}");
    let helper = format!("run_reference() {{\n{GUARD}\npython \"$@\"\n}}");
    let run = format!(
        "{EXAMPLES}\n{EVIDENCE}\n{helper}\n{}\n{GUARD}\n{CHECK}\n{}",
        REFERENCES[..8].join("\n"),
        REFERENCES[8]
    );
    let good = serde_json::json!({"jobs":{"selfdrive-runtime":{"runs-on":"ubuntu-24.04","steps":[{"run":build},{"run":run}]}}});
    check(&good).unwrap();
    for mutation in [
        "missing-build",
        "missing-check",
        "duplicate-build",
        "duplicate-check",
        "missing-build-guard",
        "missing-check-guard",
        "conditional-build",
        "conditional-check",
        "softened-build",
        "softened-check",
        "shell-softened",
        "python",
        "wrong-host",
        "optional-job",
        "wrong-example",
        "wrong-output",
        "helper",
        "unrelated-check",
    ] {
        let mut bad = good.clone();
        let job = &mut bad["jobs"]["selfdrive-runtime"];
        match mutation {
            "missing-build" => job["steps"][0]["run"] = "true".into(),
            "missing-check" => job["steps"][1]["run"] = "true".into(),
            "duplicate-build" => job["steps"][0]["run"] = format!("{build}\n{build}").into(),
            "duplicate-check" => job["steps"][1]["run"] = format!("{run}\n{run}").into(),
            "missing-build-guard" => job["steps"][0]["run"] = BUILD.into(),
            "missing-check-guard" => {
                job["steps"][1]["run"] = run.replace(&format!("{GUARD}\n{CHECK}"), CHECK).into()
            }
            "conditional-build" => job["steps"][0]["if"] = "always()".into(),
            "conditional-check" => job["steps"][1]["if"] = "always()".into(),
            "softened-build" => job["steps"][0]["continue-on-error"] = true.into(),
            "softened-check" => job["steps"][1]["continue-on-error"] = true.into(),
            "shell-softened" => {
                job["steps"][1]["run"] = run.replace(CHECK, &format!("{CHECK} || true")).into()
            }
            "python" => {
                job["steps"][1]["run"] =
                    format!("{run}\nrun_reference rust/tools/check_cutin_alerts.py").into()
            }
            "wrong-host" => job["runs-on"] = "ubuntu-24.04-arm".into(),
            "optional-job" => job["if"] = "always()".into(),
            "wrong-example" => {
                job["steps"][1]["run"] = run
                    .replace("$selfdrive_examples/cutin_alerts", "wrong/cutin_alerts")
                    .into()
            }
            "wrong-output" => {
                job["steps"][1]["run"] = run
                    .replace("$selfdrive_evidence/cutin", "wrong/cutin")
                    .into()
            }
            "helper" => {
                job["steps"][1]["run"] =
                    run.replace("python \"$@\"", "python \"$@\" || true").into()
            }
            "unrelated-check" => job["steps"][1]["run"] = run.replace(REFERENCES[0], "true").into(),
            _ => unreachable!(),
        }
        assert!(check(&bad).is_err(), "accepted {mutation}");
    }
}
