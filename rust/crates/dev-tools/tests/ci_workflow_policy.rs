//! Native counterparts of tools/ci/tests/test_workflow_policy.py.
use openpilot_dev_tools::ci_policy::{check_workflow_policy, parse, run_shell, validate_fast_gate};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn inputs() -> Vec<(String, String)> {
    [
        "fast-checks.yml",
        "integration.yml",
        "user-docs.yaml",
        "tests.yaml",
        "naver-ci.yml",
        "naver-apk-tools.yml",
        "jetlink.yml",
    ]
    .into_iter()
    .filter_map(|name| {
        let path = root().join(".github/workflows").join(name);
        path.exists()
            .then(|| (name.to_owned(), fs::read_to_string(path).unwrap()))
    })
    .collect()
}
#[test]
fn fast_checks_cover_all_branch_pushes_and_protected_prs() {
    let files = inputs();
    check_workflow_policy(&files).unwrap();
}
#[test]
fn protected_workflow_mutations_are_rejected() {
    let original = inputs();
    for (name, from, to) in [
        ("fast-checks.yml", "branches: ['**']", "branches: [dev]"),
        (
            "fast-checks.yml",
            "branches: [dev, main]",
            "branches: [main]",
        ),
        (
            "fast-checks.yml",
            "branches: ['**']",
            "branches: ['**']\n    paths: ['rust/**']",
        ),
        ("integration.yml", "${{ always() }}", "${{ success() }}"),
        ("integration.yml", "test \"$BUILD\" = success", "true"),
        ("user-docs.yaml", "- dev", "- other"),
    ] {
        let mut files = original.clone();
        let source = &mut files.iter_mut().find(|(file, _)| file == name).unwrap().1;
        assert!(
            source.contains(from),
            "mutation anchor absent: {name} {from}"
        );
        *source = source.replacen(from, to, 1);
        assert!(
            check_workflow_policy(&files).is_err(),
            "accepted {name}: {from} -> {to}"
        );
    }
}
#[test]
fn yaml_scalars_preserve_github_on_and_base_loader_values() {
    let value =
        parse("on: {push: {branches: ['**']}}\nnumber: 1\nflag: false\nquoted: '01'\n").unwrap();
    assert_eq!(value["on"]["push"]["branches"][0], "**");
    assert_eq!(value["number"], "1");
    assert_eq!(value["flag"], "false");
    assert_eq!(value["quoted"], "01");
}
#[test]
fn yaml_raw_scalars_anchors_and_multiline_match_base_loader() {
    let value = parse("on: true\nleading: 001\nhex: 0x1\nnull: null\nempty:\nquoted: '01'\ntext: |\n  first\n  second\nanchor: &settings {channel: 001, flag: true}\nalias: *settings\n").unwrap();
    assert_eq!(value["on"], "true");
    assert_eq!(value["leading"], "001");
    assert_eq!(value["hex"], "0x1");
    assert_eq!(value["null"], "null");
    assert_eq!(value["empty"], "");
    assert_eq!(value["quoted"], "01");
    assert_eq!(value["text"], "first\nsecond\n");
    assert_eq!(
        value["alias"],
        serde_json::json!({"channel":"001", "flag":"true"})
    );
}

#[test]
fn parallel_fast_gate_requires_every_worker_and_actual_success() {
    let original =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    validate_fast_gate(&original).unwrap();
    let gate = &original["jobs"]["fast"];
    let script = gate["steps"][0]["run"].as_str().unwrap();
    let success = BTreeMap::from([
        ("NATIVE".into(), "success".into()),
        ("NATIVE_CORPUS".into(), "success".into()),
        ("NATIVE_TIMING".into(), "success".into()),
        ("NATIVE_ALERTS".into(), "success".into()),
        ("NATIVE_ALERT_MANAGER".into(), "success".into()),
        ("PYTHON_ANDROID".into(), "success".into()),
    ]);
    assert!(run_shell(script, &success).unwrap().success());
    for name in [
        "NATIVE",
        "NATIVE_CORPUS",
        "NATIVE_TIMING",
        "NATIVE_ALERTS",
        "NATIVE_ALERT_MANAGER",
        "PYTHON_ANDROID",
    ] {
        for state in [
            None,
            Some(""),
            Some("failure"),
            Some("cancelled"),
            Some("skipped"),
            Some("timed_out"),
        ] {
            let mut env = success.clone();
            if let Some(state) = state {
                env.insert(name.into(), state.into());
            } else {
                env.remove(name);
            }
            assert!(
                !run_shell(script, &env).unwrap().success(),
                "accepted {name}={state:?}"
            );
        }
    }
    for dependency in [
        "native",
        "native-corpus",
        "native-timing",
        "native-alerts",
        "native-alert-manager",
        "python-android",
    ] {
        let mut missing = original.clone();
        missing["jobs"].as_object_mut().unwrap().remove(dependency);
        assert!(validate_fast_gate(&missing).is_err());
        let mut optional = original.clone();
        optional["jobs"][dependency]["if"] = "false".into();
        assert!(validate_fast_gate(&optional).is_err());
        let mut ignored = original.clone();
        ignored["jobs"][dependency]["continue-on-error"] = "true".into();
        assert!(validate_fast_gate(&ignored).is_err());
        let mut sequential = original.clone();
        sequential["jobs"][dependency]["needs"] = serde_json::json!(["other"]);
        assert!(validate_fast_gate(&sequential).is_err());
        let mut unbounded = original.clone();
        unbounded["jobs"][dependency]["timeout-minutes"] = "8".into();
        assert!(validate_fast_gate(&unbounded).is_err());
        let mut skipped_step = original.clone();
        skipped_step["jobs"][dependency]["steps"][0]["if"] = "false".into();
        assert!(validate_fast_gate(&skipped_step).is_err());
        let mut ignored_step = original.clone();
        ignored_step["jobs"][dependency]["steps"][0]["continue-on-error"] = "true".into();
        assert!(validate_fast_gate(&ignored_step).is_err());
    }
    for name in [
        "NATIVE",
        "NATIVE_CORPUS",
        "NATIVE_TIMING",
        "NATIVE_ALERTS",
        "NATIVE_ALERT_MANAGER",
        "PYTHON_ANDROID",
    ] {
        let mut missing = original.clone();
        missing["jobs"]["fast"]["steps"][0]["env"]
            .as_object_mut()
            .unwrap()
            .remove(name);
        assert!(validate_fast_gate(&missing).is_err());
        let mut wrong = original.clone();
        wrong["jobs"]["fast"]["steps"][0]["env"][name] = "success".into();
        assert!(validate_fast_gate(&wrong).is_err());
        let mut bypass = original.clone();
        bypass["jobs"]["fast"]["steps"][0]["run"] = script
            .replace(&format!("test \"${name}\" = success"), "true")
            .into();
        assert!(validate_fast_gate(&bypass).is_err());
    }
    for needs in [
        serde_json::json!(["native", "native-corpus", "native-alerts", "python-android"]),
        serde_json::json!(["native"]),
        serde_json::json!(["python-android"]),
        serde_json::json!(["native", "python-android"]),
        serde_json::json!(["native-corpus", "python-android"]),
        serde_json::json!(["native", "native-corpus"]),
        serde_json::json!(["native", "native-corpus", "python-android"]),
        serde_json::json!(["native-alerts", "native-corpus", "python-android"]),
        serde_json::json!(["native", "native-alerts", "python-android"]),
        serde_json::json!(["native", "native-corpus", "native-alerts"]),
        serde_json::json!([]),
    ] {
        let mut changed = original.clone();
        changed["jobs"]["fast"]["needs"] = needs;
        assert!(validate_fast_gate(&changed).is_err());
    }
    for (key, value) in [("if", "${{ success() }}"), ("name", "different check")] {
        let mut changed = original.clone();
        changed["jobs"]["fast"][key] = value.into();
        assert!(validate_fast_gate(&changed).is_err());
    }
}

#[test]
fn native_fast_split_preserves_every_suite_and_build_check_once() {
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    let expected = [
        "ci_workflow_policy",
        "ci_rust_isolation",
        "ci_disk_bootstrap",
        "ci_process_probe",
        "panda_oracle_build",
        "upstream_sync",
        "native_build_closure",
        "updated_process_check",
        "native_logging_build",
        "release_files",
        "logger_identifier_check",
        "qa_process",
        "logging_oracle_build",
        "path_resolve",
        "bridge_source_build",
        "jpeg_sanitizers_check",
        "ci_native_staging",
        "encoder_source_build",
        "ui_updater_handshake",
        "athena_vision_build",
        "hardware_command_check",
        "pandad_firmware_spi_build",
        "panda_cli",
        "msgq_sanitizers_check",
        "control_tools_policy_check",
        "five_cli",
        "registration_vendor_check",
        "ui_adapter_sanitizers",
        "unix_argv",
        "python_fs",
        "unix_cli_paths",
        "panda_raw",
        "startup_adapter_sanitizers",
        "python_glob",
        "ui_fixed_corpus_check",
        "ui_ci_runtime",
        "ui_state_scheduler_check",
        "ui_translation_check",
        "ui_application_logic_check",
        "ui_headless_check",
        "ui_scroll_check",
        "diagnostic_check",
        "message_state_ci",
        "ci_artifact_list",
        "miri_provenance",
        "image_array_math",
        "ui_egl_ci",
        "command_reference_check",
        "params_reference_check",
        "amplifier_policy_check",
        "hardware_control_policy_check",
        "amplifier_linux_check",
        "stats_numbers_check",
        "stats_clock_check",
        "beep_clock_check",
        "beep_sleep_check",
        "beep_integer_check",
        "beep_integer_ci",
        "ui_polygon_ci",
        "ui_application_lifecycle_ci",
        "ratekeeper_check",
        "ratekeeper_ci",
        "cutin_alert_check",
        "cutin_alert_ci",
        "bluetooth_digits_generate",
        "selfdrive_state_check",
        "alert_manager_check",
        "selfdrive_events_check",
        "alert_callbacks_check",
        "selfdrive_helpers_check",
    ];
    let corpus = [
        "amplifier_policy_check",
        "hardware_control_policy_check",
        "amplifier_linux_check",
        "stats_numbers_check",
        "stats_clock_check",
        "beep_clock_check",
        "beep_sleep_check",
        "beep_integer_check",
        "cutin_alert_check",
        "selfdrive_state_check",
    ];
    let timing = ["ratekeeper_check"];
    let alerts = [
        "selfdrive_events_check",
        "alert_callbacks_check",
        "selfdrive_helpers_check",
    ];
    let mut seen = BTreeMap::<String, usize>::new();
    let mut all_commands = Vec::new();
    for worker in [
        "native",
        "native-corpus",
        "native-timing",
        "native-alerts",
        "native-alert-manager",
    ] {
        let job = &workflow["jobs"][worker];
        assert_eq!(job["runs-on"], "ubuntu-24.04");
        for (key, value) in [
            ("RUSTUP_TOOLCHAIN", "1.94.0"),
            ("CARGO_BUILD_JOBS", "2"),
            ("CARGO_INCREMENTAL", "0"),
            ("CARGO_PROFILE_DEV_DEBUG", "0"),
            ("CARGO_PROFILE_TEST_DEBUG", "0"),
        ] {
            assert_eq!(job["env"][key], value, "{worker} {key}");
        }
        let commands = job["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|step| step["run"].as_str())
            .flat_map(|script| {
                script
                    .replace("\\\n", " ")
                    .lines()
                    .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            commands
                .iter()
                .filter(|line| line.starts_with(
                    "cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked"
                ))
                .count(),
            1,
            "{worker} must execute one native suite command"
        );
        let command = commands
            .iter()
            .find(|line| {
                line.starts_with(
                    "cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked",
                )
            })
            .unwrap();
        let args = command.split_whitespace().collect::<Vec<_>>();
        assert_eq!(args.contains(&"--lib"), worker == "native");
        let selected = args
            .windows(2)
            .filter(|pair| pair[0] == "--test")
            .map(|pair| pair[1])
            .collect::<Vec<_>>();
        let assigned = expected
            .iter()
            .copied()
            .filter(|name| match worker {
                "native-alert-manager" => *name == "alert_manager_check",
                "native-alerts" => alerts.contains(name),
                "native-corpus" => corpus.contains(name),
                "native-timing" => timing.contains(name),
                _ => {
                    !corpus.contains(name)
                        && !timing.contains(name)
                        && !alerts.contains(name)
                        && *name != "alert_manager_check"
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(selected, assigned, "{worker} suite selection changed");
        for suite in selected {
            *seen.entry(suite.to_owned()).or_default() += 1;
        }
        assert_eq!(commands.iter().filter(|line| line.as_str() ==
            "\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"
        ).count(), 1, "{worker} disk guard changed");
        all_commands.extend(commands);
    }
    assert_eq!(seen.len(), expected.len());
    assert!(seen.values().all(|count| *count == 1));
    for command in [
        "python -m unittest discover -s tools/ci/tests -v",
        "cargo test --manifest-path rust/Cargo.toml -p openpilot-service-tools --locked",
    ] {
        assert_eq!(
            all_commands
                .iter()
                .filter(|line| line.as_str() == command)
                .count(),
            1,
            "dropped or duplicated {command}"
        );
    }
}
