//! Native counterparts of tools/ci/tests/test_workflow_policy.py.
use openpilot_dev_tools::ci_policy::{
    check_workflow_policy, parse, run_shell, validate_fast_gate, validate_local_routing,
    LOCAL_RUNNER, TRUSTED_SOURCE,
};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn inputs() -> Vec<(String, String)> {
    fs::read_dir(root().join(".github/workflows"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .map(|path| {
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                fs::read_to_string(path).unwrap(),
            )
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
        (
            "fast-checks.yml",
            "cancel-in-progress: false",
            "cancel-in-progress: true",
        ),
        ("fast-checks.yml", "queue: max", "queue: single"),
        ("integration.yml", "${{ always() }}", "${{ success() }}"),
        ("integration.yml", "test \"$BUILD\" = success", "true"),
        (
            "user-docs.yaml",
            "branches: [carrot-wip, dev, main]",
            "branches: [carrot-wip, other, main]",
        ),
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
        ("SOURCE_ALLOWED".into(), "true".into()),
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
        "timed_ci",
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
        "encoder_runtime_ci",
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
        assert_eq!(job["runs-on"], serde_json::json!(LOCAL_RUNNER));
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

#[test]
fn future_diagnostic_runner_and_fork_refusal_cannot_use_hosted_fallback() {
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    validate_local_routing(&workflow).unwrap();
    for runner in [
        serde_json::json!("ubuntu-24.04"),
        serde_json::json!("ubuntu-24.04-arm"),
        serde_json::json!("${{ matrix.runner }}"),
    ] {
        let mut bad = workflow.clone();
        bad["jobs"]["native"]["runs-on"] = runner;
        assert!(validate_local_routing(&bad).is_err());
    }
    let mut bad = workflow.clone();
    bad["on"]["pull_request_target"] = serde_json::json!({});
    assert!(validate_local_routing(&bad).is_err());
    let mut bad = workflow.clone();
    bad["jobs"]["native"]["steps"][0]["continue-on-error"] = "true".into();
    assert!(validate_local_routing(&bad).is_err());
    let mut bad = workflow.clone();
    bad["jobs"]["native"]["steps"][0]["env"]["SOURCE_ALLOWED"] = "true".into();
    assert!(validate_local_routing(&bad).is_err());
    let mut bad = workflow.clone();
    bad["jobs"]["native"]["steps"][1]["if"] = "always()".into();
    assert!(validate_local_routing(&bad).is_err());
    let mut escaped = workflow.clone();
    escaped["jobs"]["native"]["steps"][1]["if"] = format!(
        "${{{{ ({}) && (false) || always() }}}}",
        TRUSTED_SOURCE
            .trim_start_matches("${{ ")
            .trim_end_matches(" }}")
    )
    .into();
    assert!(validate_local_routing(&escaped).is_err());
    let gate = &workflow["jobs"]["fast"];
    let script = gate["steps"][0]["run"].as_str().unwrap();
    assert_eq!(gate["steps"][0]["env"]["SOURCE_ALLOWED"], TRUSTED_SOURCE);
    for state in [None, Some(""), Some("false"), Some("skipped")] {
        let mut values = BTreeMap::from([
            ("NATIVE".into(), "success".into()),
            ("NATIVE_CORPUS".into(), "success".into()),
            ("NATIVE_TIMING".into(), "success".into()),
            ("NATIVE_ALERTS".into(), "success".into()),
            ("NATIVE_ALERT_MANAGER".into(), "success".into()),
            ("PYTHON_ANDROID".into(), "success".into()),
        ]);
        if let Some(state) = state {
            values.insert("SOURCE_ALLOWED".into(), state.into());
        }
        assert!(
            !run_shell(script, &values).unwrap().success(),
            "fork must fail, not pass skipped checks"
        );
    }
}

#[test]
fn common_foundation_files_match_the_separate_pinned_source_contract() {
    use sha2::{Digest, Sha256};
    let contract: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root().join("tools/ci/local-routing-296-contract.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        contract["source_commit"],
        "77663836a95a9e2aff9aa66592d8b06ed9c87cd9"
    );
    assert_eq!(contract["files"].as_object().unwrap().len(), 4);
    for (name, seal) in contract["files"].as_object().unwrap() {
        let bytes = fs::read(root().join(name)).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            seal["sha256"].as_str().unwrap(),
            "common interface drift: {name}"
        );
    }
    let source = fs::read_to_string(root().join("tools/ci/local-arm")).unwrap();
    assert!(source.contains("physical_arm_cpu\":false"));
    assert!(source.contains("[[ $guest_machine == aarch64 ]]"));
    assert!(source.contains("[[ $(git -C \"$source_dir\" rev-parse HEAD) == \"$GITHUB_SHA\" ]]"));
}

#[test]
fn integration_aggregate_cannot_pass_failed_or_skipped_workers_with_a_trailing_true() {
    let mut files = inputs();
    let source = &mut files
        .iter_mut()
        .find(|(name, _)| name == "integration.yml")
        .unwrap()
        .1;
    assert!(source.contains("test \"$JETLINK\" = success"));
    *source = source.replacen(
        "test \"$JETLINK\" = success",
        "test \"$JETLINK\" = success\n          true",
        1,
    );
    assert!(check_workflow_policy(&files).is_err());
}

#[test]
fn diagnostic_fast_slice_keeps_private_paths_and_explicit_fork_failure() {
    use openpilot_dev_tools::ci_policy::DIAGNOSTIC_TRUSTED_SOURCE;
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    validate_fast_gate(&workflow).unwrap();
    for (name, worker) in workflow["jobs"].as_object().unwrap() {
        assert_eq!(worker["runs-on"], serde_json::json!(LOCAL_RUNNER));
        if name == "fast" {
            continue;
        }
        let steps = worker["steps"].as_array().unwrap();
        let allocate = steps
            .iter()
            .position(|s| {
                s["name"] == "Allocate private Fast source cache and target before checkout"
            })
            .unwrap();
        let checkout = steps
            .iter()
            .position(|s| {
                s["uses"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("actions/checkout@")
            })
            .unwrap();
        assert!(
            allocate < checkout,
            "source growth/allocation must precede checkout"
        );
        let body = steps[allocate]["run"].as_str().unwrap();
        for marker in [
            "2 * 1024 * 1024 * 1024",
            "mktemp -d",
            "CARGO_TARGET_DIR=$source/rust/target",
            "CARGO_HOME=$job/cargo-home",
            "RUSTUP_HOME=$job/rustup-home",
            "GRADLE_USER_HOME=$job/cache/gradle",
            "DIAG_ACTIONS_WORKSPACE=$workspace",
            "if [[ ${GITHUB_WORKSPACE:-} == %q ]]; then",
        ] {
            assert!(
                body.contains(marker),
                "missing private bootstrap field {marker}"
            );
        }
        assert!(
            body.find("((available >= required))").unwrap()
                < body.find("source_owner=$(mktemp").unwrap()
        );
        assert_eq!(
            steps[checkout]["with"]["path"],
            "${{ env.DIAG_CHECKOUT_PATH }}"
        );
        assert_eq!(steps[checkout]["with"]["fetch-depth"], "0");
        for field in ["persist-credentials", "set-safe-directory", "clean", "lfs"] {
            assert_eq!(steps[checkout]["with"][field], "false");
        }
        let clean = steps
            .iter()
            .find(|s| s["name"] == "Clean only uploaded private Fast source")
            .unwrap();
        let body = clean["run"].as_str().unwrap();
        for marker in [
            "UPLOAD_ID =~",
            "UPLOAD_DIGEST =~",
            "stat -c '%d:%i'",
            "test ! -L",
            ".diag296-owner",
            "rm -rf --one-file-system -- \"$p\"",
        ] {
            assert!(
                body.contains(marker),
                "unsafe owned cleanup missing {marker}"
            );
        }
        let mut hosted = workflow.clone();
        hosted["jobs"][name]["runs-on"] = "ubuntu-24.04".into();
        assert!(validate_fast_gate(&hosted).is_err());
        assert_eq!(steps[0]["env"]["SOURCE_ALLOWED"], DIAGNOSTIC_TRUSTED_SOURCE);
    }
    let mut fork_pass = workflow.clone();
    fork_pass["jobs"]["fast"]["steps"][0]["env"]["SOURCE_ALLOWED"] = "true".into();
    assert!(validate_fast_gate(&fork_pass).is_err());
}

#[test]
fn diagnostic_common_helpers_match_verified_7766383_bytes_without_reexecution() {
    use sha2::{Digest, Sha256};
    let contract: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root().join("tools/ci/diagnostic-296-contract.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        contract["common_source"],
        "77663836a95a9e2aff9aa66592d8b06ed9c87cd9"
    );
    assert_eq!(contract["common_files"].as_object().unwrap().len(), 4);
    for (name, seal) in contract["common_files"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root().join(name)).unwrap())),
            seal["sha256"].as_str().unwrap(),
            "common source drift {name}"
        );
    }
}

#[test]
fn full_private_jobs_reject_shared_checkouts_or_unsealed_cleanup() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflows = inputs();
    let mut covered = 0;
    for (_, source) in workflows {
        let workflow = parse(&source).unwrap();
        for (name, job) in workflow["jobs"].as_object().unwrap() {
            if job["env"].get("CI296_ALIAS_BASENAME").is_none() {
                continue;
            }
            validate_private_job_layout(job).unwrap_or_else(|e| panic!("{name}: {e}"));
            covered += 1;
            let steps = job["steps"].as_array().unwrap();
            let checkout = steps
                .iter()
                .position(|s| {
                    s["uses"]
                        .as_str()
                        .unwrap_or("")
                        .starts_with("actions/checkout@")
                })
                .unwrap();
            let mut shared = job.clone();
            shared["steps"][checkout]["with"]["path"] = ".".into();
            assert!(validate_private_job_layout(&shared).is_err());
            let cleanup = steps
                .iter()
                .position(|s| s["name"] == "Clean only uploaded owned source alias and target")
                .unwrap();
            let mut unsealed = job.clone();
            unsealed["steps"][cleanup]["if"] = "${{ always() }}".into();
            assert!(validate_private_job_layout(&unsealed).is_err());
            let allocate=steps.iter().position(|s|s["name"]=="Allocate private source cache target and local action alias before checkout").unwrap();
            let mut unsafe_alias = job.clone();
            unsafe_alias["steps"][allocate]["run"] = steps[allocate]["run"]
                .as_str()
                .unwrap()
                .replace("ln -sT", "ln -sf")
                .into();
            assert!(validate_private_job_layout(&unsafe_alias).is_err());
        }
    }
    assert_eq!(covered, 60, "maintained private source roster changed");
}

#[test]
fn every_existing_concurrency_scope_preserves_running_and_pending_jobs() {
    use openpilot_dev_tools::ci_policy::validate_concurrency;
    let mut covered = 0;
    for (name, text) in inputs() {
        let original = parse(&text).unwrap();
        validate_concurrency(&original).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut scopes = Vec::new();
        if original.get("concurrency").is_some() {
            scopes.push(None);
        }
        for (job, value) in original["jobs"].as_object().unwrap() {
            if value.get("concurrency").is_some() {
                scopes.push(Some(job.clone()));
            }
        }
        for job in scopes {
            covered += 1;
            for mutation in ["missing", "single", "cancels"] {
                let mut changed = original.clone();
                let concurrency = match &job {
                    Some(job) => &mut changed["jobs"][job]["concurrency"],
                    None => &mut changed["concurrency"],
                };
                match mutation {
                    "missing" => {
                        concurrency.as_object_mut().unwrap().remove("queue");
                    }
                    "single" => concurrency["queue"] = "single".into(),
                    "cancels" => concurrency["cancel-in-progress"] = "true".into(),
                    _ => unreachable!(),
                }
                assert!(
                    validate_concurrency(&changed).is_err(),
                    "{name} {job:?}: {mutation}"
                );
            }
        }
    }
    assert!(covered >= 10, "concurrency roster unexpectedly shrank");
    // Exercise job-scoped queues even when the maintained workflows currently
    // use workflow scope; the separately published focused recipe uses this.
    for scope in [
        "concurrency: {group: fixed, cancel-in-progress: false, queue: max}\njobs: {}\n",
        "jobs: {focus: {concurrency: {group: fixed, cancel-in-progress: false, queue: max}}}\n",
    ] {
        validate_concurrency(&parse(scope).unwrap()).unwrap();
        for (from, to) in [
            ("queue: max", "queue: single"),
            ("cancel-in-progress: false", "cancel-in-progress: true"),
        ] {
            assert!(validate_concurrency(&parse(&scope.replace(from, to)).unwrap()).is_err());
        }
    }
}

#[test]
fn arm_target_steps_and_nested_native_actions_require_explicit_command_file_shell() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap();
    let mut routed = 0;
    let mut calls = Vec::new();
    for (name, job) in workflow["jobs"].as_object().unwrap() {
        if job["env"].get("LOCAL_CI_ARCH").is_none() {
            continue;
        }
        routed += 1;
        validate_private_job_layout(job).unwrap();
        for (index, step) in job["steps"].as_array().unwrap().iter().enumerate() {
            if step.get("run").is_some() && step["env"]["CI296_HOST_CONTROL"] != "1" {
                let mut bypass = job.clone();
                bypass["steps"][index]["shell"] = "bash".into();
                assert!(
                    validate_private_job_layout(&bypass).is_err(),
                    "{name}: target Bash bypass"
                );
            }
            if step["uses"].as_str().is_some_and(|called| {
                ["native-json11", "native-eigen", "native-image-sdk"]
                    .iter()
                    .any(|action| called.ends_with(&format!("/.github/actions/{action}")))
            }) {
                calls.push(name.as_str());
                for shell in [None, Some("bash")] {
                    let mut bypass = job.clone();
                    match shell {
                        Some(shell) => bypass["steps"][index]["with"]["run-shell"] = shell.into(),
                        None => {
                            bypass["steps"][index]["with"]
                                .as_object_mut()
                                .unwrap()
                                .remove("run-shell");
                        }
                    }
                    assert!(
                        validate_private_job_layout(&bypass).is_err(),
                        "{name}: composite shell input bypass"
                    );
                }
            }
        }
    }
    assert_eq!(routed, 8);
    calls.sort_unstable();
    assert_eq!(
        calls,
        [
            "carrot-man-runtime",
            "planner-runtime",
            "radar-arm",
            "radard-runtime",
            "webrtc-runtime",
            "xiaoge-runtime"
        ]
    );
}

#[test]
fn native_action_selection_is_bound_to_the_same_private_job_source() {
    use openpilot_dev_tools::ci_policy::uses_native_action;
    let job =
        serde_json::json!({"env":{"CI296_ALIAS_BASENAME":".ci296-source-rust--planner-runtime"}});
    let good = serde_json::json!({"uses":"./.ci296-source-rust--planner-runtime/source/.github/actions/native-json11"});
    assert!(uses_native_action(&job, &good, "native-json11"));
    for called in [
        "./.github/actions/native-json11",
        "./.ci296-source-rust--radard-runtime/source/.github/actions/native-json11",
        "./.ci296-source-rust--planner-runtime/source/.github/actions/native-image-sdk",
    ] {
        assert!(!uses_native_action(
            &job,
            &serde_json::json!({"uses":called}),
            "native-json11"
        ));
    }
    assert!(uses_native_action(
        &serde_json::json!({}),
        &serde_json::json!({"uses":"./.github/actions/native-json11"}),
        "native-json11"
    ));
}

#[test]
fn private_ownership_exact_guards_reject_missing_seal_or_admission() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let (mut sources, mut arm) = (0, 0);
    for (_, text) in inputs() {
        let workflow = parse(&text).unwrap();
        for (name, job) in workflow["jobs"].as_object().unwrap() {
            if job["env"].get("CI296_ALIAS_BASENAME").is_none() {
                continue;
            }
            validate_private_job_layout(job).unwrap();
            sources += 1;
            let steps = job["steps"].as_array().unwrap();
            let at = |name: &str| steps.iter().position(|s| s["name"] == name).unwrap();
            let seal = at("Seal private source and index after maintained checks");
            let cleanup = at("Clean only uploaded owned source alias and target");
            let common = at("Clean common owned temporary paths after uploaded source");
            let original = steps[cleanup]["if"].as_str().unwrap();
            for condition in [
                original.replace(" && steps.ci296_source_seal.outcome == 'success'", ""),
                original.replace("always() &&", "always() || true ||"),
                original.replace("ci296_source_seal.outcome", "other_seal.outcome"),
            ] {
                assert_ne!(condition, original);
                let mut changed = job.clone();
                changed["steps"][cleanup]["if"] = condition.into();
                assert!(
                    validate_private_job_layout(&changed).is_err(),
                    "{name}: incomplete source seal may delete proof"
                );
            }
            let mut changed = job.clone();
            changed["steps"][seal]["id"] = "unbound_seal".into();
            assert!(validate_private_job_layout(&changed).is_err());
            let mut changed = job.clone();
            changed["steps"][seal]["run"] = "true".into();
            assert!(validate_private_job_layout(&changed).is_err());
            let mut changed = job.clone();
            changed["steps"][common]["if"] = steps[common]["if"]
                .as_str()
                .unwrap()
                .replace(" && steps.ci296_source_cleanup.outcome == 'success'", "")
                .into();
            assert!(
                validate_private_job_layout(&changed).is_err(),
                "{name}: common proof cleanup bypass"
            );
            if job["env"].get("LOCAL_CI_ARCH").is_some() {
                arm += 1;
                let admission = at("Admit exact private source and index");
                let finish = at("Finish emulated architecture evidence");
                let mut changed = job.clone();
                changed["steps"][finish]["if"] = steps[finish]["if"]
                    .as_str()
                    .unwrap()
                    .replace(" && steps.ci296_source_admission.outcome == 'success'", "")
                    .into();
                assert!(
                    validate_private_job_layout(&changed).is_err(),
                    "{name}: stale alias finalizer"
                );
                let mut changed = job.clone();
                changed["steps"][admission]["id"] = "unbound_admission".into();
                assert!(validate_private_job_layout(&changed).is_err());
            }
        }
    }
    assert_eq!((sources, arm), (60, 8));
}

// Evaluate the actual restricted workflow condition, not a separately invented
// cleanup predicate. These controls run only in the parent-published Actions gate.
// Shared transport and provider actions are never invoked by these tests.
fn ownership_condition_as_bash(condition: &str, outcomes: &BTreeMap<String, String>) -> String {
    let expression = condition
        .strip_prefix("${{ ")
        .unwrap()
        .strip_suffix(" }}")
        .unwrap();
    let expression = expression
        .replace("github.event_name", "'push'")
        .replace(
            "github.event.pull_request.head.repo.full_name",
            "'bin9208/openpilot-rust'",
        )
        .replace("github.repository", "'bin9208/openpilot-rust'")
        .replace("env.CI296_SOURCE_OWNER", "'owned-fixture-source'")
        .replace("always()", "1 == 1");
    let tokens = regex::Regex::new(
        r"steps\.([A-Za-z0-9_-]+)\.(outcome|outputs\.artifact-id|outputs\.artifact-digest)",
    )
    .unwrap();
    let expression = tokens
        .replace_all(&expression, |caps: &regex::Captures<'_>| {
            let value = if &caps[2] == "outcome" {
                outcomes
                    .get(&caps[1])
                    .map(String::as_str)
                    .unwrap_or("success")
            } else if &caps[2] == "outputs.artifact-id" {
                "123"
            } else {
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            };
            format!("'{value}'")
        })
        .into_owned();
    for unresolved in ["github.", "steps.", "env."] {
        assert!(!expression.contains(unresolved));
    }
    expression
}

#[test]
fn private_failed_seal_retains_source_alias_and_common_proof() {
    use std::os::unix::fs::{symlink, MetadataExt};
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/naver-ci.yml")).unwrap()).unwrap();
    let job = workflow["jobs"]
        .as_object()
        .unwrap()
        .values()
        .find(|job| job["env"].get("CI296_ALIAS_BASENAME").is_some())
        .unwrap();
    let steps = job["steps"].as_array().unwrap();
    let source_cleanup = steps
        .iter()
        .find(|s| s["name"] == "Clean only uploaded owned source alias and target")
        .unwrap();
    let common_cleanup = steps
        .iter()
        .find(|s| s["name"] == "Clean common owned temporary paths after uploaded source")
        .unwrap();
    for seal in ["failure", "cancelled", "skipped", "", "success"] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let workspace = fs::canonicalize(workspace).unwrap();
        let owner = workspace.join(".ci296-source-123-1.ABCDEF");
        let source = owner.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(owner.join(".ci296-owner"), "123:1:ownership-fixture").unwrap();
        fs::write(
            source.join("held-fixture-input"),
            b"held original fixture bytes",
        )
        .unwrap();
        let alias_name = job["env"]["CI296_ALIAS_BASENAME"].as_str().unwrap();
        let alias = workspace.join(alias_name);
        symlink(&owner, &alias).unwrap();
        let identity = |path: &std::path::Path| {
            let m = fs::symlink_metadata(path).unwrap();
            format!("{}:{}", m.dev(), m.ino())
        };
        let proof = temp.path().join("proof");
        fs::create_dir(&proof).unwrap();
        fs::write(
            proof.join("partial-uploaded-receipt"),
            b"uploaded partial proof is insufficient",
        )
        .unwrap();
        let env = BTreeMap::from([
            ("GITHUB_WORKSPACE".into(), workspace.display().to_string()),
            (
                "CI296_ACTIONS_WORKSPACE".into(),
                workspace.display().to_string(),
            ),
            ("CI296_SOURCE_OWNER".into(), owner.display().to_string()),
            ("CI296_SOURCE".into(), source.display().to_string()),
            ("CI296_SOURCE_IDENTITY".into(), identity(&owner)),
            ("CI296_SOURCE_ALIAS".into(), alias.display().to_string()),
            ("CI296_ALIAS_BASENAME".into(), alias_name.into()),
            ("CI296_ALIAS_IDENTITY".into(), identity(&alias)),
            ("GITHUB_RUN_ID".into(), "123".into()),
            ("GITHUB_RUN_ATTEMPT".into(), "1".into()),
            ("GITHUB_JOB".into(), "ownership-fixture".into()),
            ("UPLOAD_ID".into(), "123".into()),
            ("UPLOAD_DIGEST".into(), "a".repeat(64)),
        ]);
        let mut outcomes = BTreeMap::from([("ci296_source_seal".into(), seal.into())]);
        if seal == "skipped" {
            outcomes.insert("ci296_source_admission".into(), "failure".into());
        }
        let expression =
            ownership_condition_as_bash(source_cleanup["if"].as_str().unwrap(), &outcomes);
        let script = format!(
            "if [[ {expression} ]]; then\n{}\nelse\nexit 17\nfi\n",
            source_cleanup["run"].as_str().unwrap()
        );
        let status = run_shell(&script, &env).unwrap();
        if seal == "success" {
            assert!(status.success());
            assert!(!owner.exists() && !alias.exists());
        } else {
            assert_eq!(status.code(), Some(17));
            assert_eq!(
                fs::read(source.join("held-fixture-input")).unwrap(),
                b"held original fixture bytes"
            );
            assert_eq!(fs::read_link(&alias).unwrap(), owner);
            outcomes.insert("ci296_source_cleanup".into(), "skipped".into());
            let common =
                ownership_condition_as_bash(common_cleanup["if"].as_str().unwrap(), &outcomes);
            assert_eq!(
                run_shell(
                    &format!("if [[ {common} ]]; then exit 0; else exit 17; fi"),
                    &env
                )
                .unwrap()
                .code(),
                Some(17)
            );
            assert!(proof.join("partial-uploaded-receipt").is_file());
        }
    }
}

#[test]
fn private_arm_finalizer_requires_fresh_admission_before_alias_load() {
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap();
    let mut covered = 0;
    for (name, job) in workflow["jobs"].as_object().unwrap() {
        if job["env"].get("LOCAL_CI_ARCH").is_none() {
            continue;
        }
        covered += 1;
        let finish = job["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "Finish emulated architecture evidence")
            .unwrap();
        for admission in ["failure", "cancelled", "skipped", "", "success"] {
            let temp = tempfile::tempdir().unwrap();
            let retained = temp.path().join("retained-old-action.yml");
            fs::write(&retained, b"inert retained action manifest").unwrap();
            let probe = temp.path().join("manifest-load-probe");
            let outcomes = BTreeMap::from([("ci296_source_admission".into(), admission.into())]);
            let condition = ownership_condition_as_bash(finish["if"].as_str().unwrap(), &outcomes);
            let env = BTreeMap::from([
                ("RETAINED_MANIFEST".into(), retained.display().to_string()),
                ("LOAD_PROBE".into(), probe.display().to_string()),
            ]);
            let script = format!("if [[ {condition} ]]; then test -f \"$RETAINED_MANIFEST\"; printf admitted > \"$LOAD_PROBE\"; else exit 17; fi");
            let status = run_shell(&script, &env).unwrap();
            assert_eq!(
                status.success(),
                admission == "success",
                "{name}/{admission}"
            );
            assert_eq!(probe.exists(), admission == "success");
            assert_eq!(
                fs::read(retained).unwrap(),
                b"inert retained action manifest"
            );
        }
    }
    assert_eq!(covered, 8);
}

#[test]
fn private_android_sdk_root_rejects_inherited_or_shared_storage() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/naver-apk-tools.yml")).unwrap())
            .unwrap();
    let mut covered = 0;
    for (name, job) in workflow["jobs"].as_object().unwrap() {
        validate_private_job_layout(job).unwrap();
        for (index, step) in job["steps"].as_array().unwrap().iter().enumerate() {
            if !step["uses"]
                .as_str()
                .is_some_and(|called| called.starts_with("android-actions/setup-android@"))
            {
                continue;
            }
            covered += 1;
            for path in [
                None,
                Some("${{ env.ANDROID_SDK_ROOT }}"),
                Some("/opt/android-sdk"),
            ] {
                let mut changed = job.clone();
                match path {
                    Some(path) => changed["steps"][index]["env"]["ANDROID_SDK_ROOT"] = path.into(),
                    None => {
                        changed["steps"][index]["env"]
                            .as_object_mut()
                            .unwrap()
                            .remove("ANDROID_SDK_ROOT");
                    }
                }
                assert!(
                    validate_private_job_layout(&changed).is_err(),
                    "{name}: inherited/shared SDK root"
                );
            }
        }
    }
    assert_eq!(covered, 2);
}
