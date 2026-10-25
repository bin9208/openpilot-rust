//! Native counterparts of tools/ci/tests/test_workflow_policy.py.
use openpilot_dev_tools::ci_policy::{
    check_workflow_policy, parse, run_shell, validate_fast_gate, validate_local_routing,
    HOSTED_RUNNER, LOCAL_RUNNER, TRUSTED_SOURCE,
};
use std::{collections::BTreeMap, fs, path::PathBuf};

const CHANGED_FILES_REGRESSION: &str = "cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --no-default-features --features changed-files-native --test changed_files_caller";
const NAVI_POLICY: [&str; 2] = [
    "cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --features carrot-navi-compare-native --test carrot_navi_compare maintained_policy_rejects_embedded_oracle_and_changed_traffic -- --exact",
    "cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --features carrot-navi-compare-native --test carrot_navi_compare maintained_policy_accepts_yaml_aliases_and_rejects_unreviewed_overlays -- --exact",
];

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
fn maintained_workflow_files_fit_github_registration_byte_limit() {
    for (name, source) in inputs() {
        // String::len measures UTF-8 bytes, which GitHub admits before job creation.
        let bytes = source.len();
        assert!(
            bytes <= 500_000,
            "{name}: {bytes} bytes exceeds the 500000-byte workflow registration limit"
        );
    }
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
    // Separate an alias key from ':' so the scanner does not consume the
    // delimiter as part of the anchor name (the maintained flow-YAML case).
    let keys =
        parse("{first: {&field working-directory: &folder source}, second: {*field : *folder}}")
            .unwrap();
    assert_eq!(keys["first"]["working-directory"], "source");
    assert_eq!(keys["second"], keys["first"]);
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
        ("NATIVE_TOOLS".into(), "success".into()),
        ("NATIVE_CORPUS".into(), "success".into()),
        ("NATIVE_TIMING".into(), "success".into()),
        ("NATIVE_ALERTS".into(), "success".into()),
        ("NATIVE_ALERT_MANAGER".into(), "success".into()),
        ("PYTHON_ANDROID".into(), "success".into()),
    ]);
    assert!(run_shell(script, &success).unwrap().success());
    for name in [
        "NATIVE",
        "NATIVE_TOOLS",
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
        "native-tools",
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
        "NATIVE_TOOLS",
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
        let mut crossed = original.clone();
        let other = if name == "NATIVE" {
            "native-tools"
        } else {
            "native"
        };
        crossed["jobs"]["fast"]["steps"][0]["env"][name] =
            format!("${{{{ needs.{other}.result }}}}").into();
        assert!(validate_fast_gate(&crossed).is_err());
        let mut bypass = original.clone();
        bypass["jobs"]["fast"]["steps"][0]["run"] = script
            .replace(&format!("test \"${name}\" = success"), "true")
            .into();
        assert!(validate_fast_gate(&bypass).is_err());
    }
    for needs in [
        serde_json::json!([
            "native",
            "native-corpus",
            "native-timing",
            "native-alerts",
            "native-alert-manager",
            "python-android"
        ]),
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
    let policy = [
        "ci_workflow_policy",
        "ci_rust_isolation",
        "ci_disk_bootstrap",
        "ci_process_probe",
    ];
    let cold_audit = "CI_PREPARED_RUST_PROOF=\"$CI_JOB_TEMP/proof\" cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test ci_disk_bootstrap prepared_audit::prepared_audit_actual_cold_bootstrap_receipt -- --ignored --exact --nocapture 2>&1 | tee \"$CI_JOB_TEMP/proof/prepared-cold-native-audit.log\"";
    let caller_prepare = "\"$GITHUB_WORKSPACE/rust/tools/prepare-changed-files-caller\" \"$CI_JOB_TEMP/proof/changed-files-caller\" \"$CI_JOB_TEMP/changed-files-caller.env\"";
    let caller_context = "source \"$CI_JOB_TEMP/changed-files-caller.env\"";
    let service = "cargo test --manifest-path rust/Cargo.toml -p openpilot-service-tools --locked";
    let owned_commands = [
        (cold_audit, "native"),
        (
            "grep -F 'test result: ok. 1 passed;' \"$CI_JOB_TEMP/proof/prepared-cold-native-audit.log\"",
            "native",
        ),
        (caller_prepare, "native"),
        (caller_context, "native"),
        (CHANGED_FILES_REGRESSION, "native"),
        (NAVI_POLICY[0], "native"),
        (NAVI_POLICY[1], "native"),
        (service, "native-tools"),
    ];
    let mut seen = BTreeMap::<String, usize>::new();
    let mut all_commands = Vec::new();
    let mut native_split = Vec::new();
    for worker in [
        "native",
        "native-tools",
        "native-corpus",
        "native-timing",
        "native-alerts",
        "native-alert-manager",
    ] {
        let job = &workflow["jobs"][worker];
        assert_eq!(job["runs-on"], serde_json::json!(HOSTED_RUNNER));
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
                .filter(|line| {
                    line.as_str() != CHANGED_FILES_REGRESSION
                    && !NAVI_POLICY.contains(&line.as_str()) && line.starts_with(
                        "cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked"
                    )
                })
                .count(),
            1,
            "{worker} must execute one baseline native suite command"
        );
        let command = commands
            .iter()
            .find(|line| {
                line.as_str() != CHANGED_FILES_REGRESSION
                && !NAVI_POLICY.contains(&line.as_str()) && line.starts_with(
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
                "native" => policy.contains(name),
                "native-tools" => {
                    !corpus.contains(name)
                        && !timing.contains(name)
                        && !alerts.contains(name)
                        && *name != "alert_manager_check"
                        && !policy.contains(name)
                }
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(selected, assigned, "{worker} suite selection changed");
        let mut exact =
            "cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked".to_owned();
        if worker == "native" {
            exact.push_str(" --lib");
        }
        for suite in &assigned {
            exact.push_str(" --test ");
            exact.push_str(suite);
        }
        assert_eq!(
            command, &exact,
            "{worker} baseline command signature changed"
        );
        if matches!(worker, "native" | "native-tools") {
            assert_eq!(selected.len(), if worker == "native" { 4 } else { 53 });
            native_split.extend(selected.iter().map(|suite| (*suite).to_owned()));
        }
        for suite in selected {
            *seen.entry(suite.to_owned()).or_default() += 1;
        }
        let disk_guard = "\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048";
        let disk_guard = if worker == "native" {
            format!("OPENPILOT_CI_PREPARED_RUST_ROOT=\"$CI_PREPARED_RUST_ROOT\" OPENPILOT_CI_PREPARED_RUST_MANIFEST=\"$CI_PREPARED_RUST_MANIFEST\" exec /usr/bin/strace --debug --seccomp-bpf -f -qq -s 65535 -e trace=execve,execveat -o \"$CI_JOB_TEMP/proof/prepared-cold-bootstrap.exec\" {disk_guard}")
        } else {
            format!("OPENPILOT_CI_PREPARED_RUST_ROOT=\"$CI_PREPARED_RUST_ROOT\" OPENPILOT_CI_PREPARED_RUST_MANIFEST=\"$CI_PREPARED_RUST_MANIFEST\" {disk_guard}")
        };
        assert_eq!(
            commands.iter().filter(|line| *line == &disk_guard).count(),
            1,
            "{worker} disk guard changed"
        );
        let operation_guard = "OPENPILOT_CI_PREPARED_RUST_ROOT=\"$CI_PREPARED_RUST_ROOT\" OPENPILOT_CI_PREPARED_RUST_MANIFEST=\"$CI_PREPARED_RUST_MANIFEST\" \"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib 6144";
        let split_worker = matches!(worker, "native" | "native-tools");
        assert_eq!(
            commands
                .iter()
                .filter(|line| line.as_str() == operation_guard)
                .count(),
            usize::from(split_worker),
            "{worker} operation admission changed"
        );
        for (owned, owner) in owned_commands {
            assert_eq!(
                commands
                    .iter()
                    .filter(|line| line.as_str() == owned)
                    .count(),
                usize::from(worker == owner),
                "{worker}: dropped, duplicated or misplaced {owned}"
            );
        }
        let at = |expected: &str| {
            commands
                .iter()
                .position(|line| line.as_str() == expected)
                .unwrap()
        };
        if split_worker {
            assert!(at(&disk_guard) < at(operation_guard));
            assert!(at(operation_guard) < at(command));
        }
        if worker == "native" {
            assert!(at(operation_guard) < at(cold_audit));
            assert!(at(cold_audit) < at(caller_prepare));
            assert!(at(caller_prepare) < at(caller_context));
            assert!(at(caller_context) < at(CHANGED_FILES_REGRESSION));
            assert!(at(CHANGED_FILES_REGRESSION) < at(command));
            assert!(at(command) < at(NAVI_POLICY[0]));
            assert!(at(NAVI_POLICY[0]) < at(NAVI_POLICY[1]));
        } else if worker == "native-tools" {
            assert!(at(command) < at(service));
        }
        all_commands.extend(commands);
    }
    assert_eq!(expected.len(), 72);
    assert_eq!(seen.len(), expected.len());
    assert!(seen.values().all(|count| *count == 1));
    let original_native = expected
        .iter()
        .copied()
        .filter(|name| {
            !corpus.contains(name)
                && !timing.contains(name)
                && !alerts.contains(name)
                && *name != "alert_manager_check"
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(native_split.len(), 57);
    assert_eq!(native_split, original_native);
    for command in [
        CHANGED_FILES_REGRESSION,
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
fn cold_bootstrap_stderr_collection_is_owned_bounded_and_preserves_failure_evidence() {
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    let script = workflow["jobs"]["native"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["name"] == "Prepare native CI validation")
        .unwrap()["run"]
        .as_str()
        .unwrap();
    let status_at = script.find("> \"$cold_proof/cold-tracer.status\"").unwrap();
    let collect_at = script.find("cold_collect_stderr 100 20").unwrap();
    assert!(
        status_at < collect_at,
        "persist tracer status before draining"
    );
    let functions = script
        .split_once("cold_collector_current() {\n")
        .unwrap()
        .1
        .split_once("cold_provider before\n")
        .unwrap()
        .0;
    let launch = script
        .split_once("exec {cold_stderr_fd}> >(\n")
        .unwrap()
        .1
        .split_once("cold_status=0\n")
        .unwrap()
        .0;
    let finish = script.split_once("cold_records_status=0\n").unwrap().1;
    assert_eq!(launch.matches("ulimit -f 524288 || exit 1").count(), 1);
    // Actions-only: reuse the real collector and finalization code. Shorten only
    // its private poll counts; no production environment override is introduced.
    assert_eq!(finish.matches("cold_collect_stderr 100 20").count(), 1);
    let finish = finish.replace("cold_collect_stderr 100 20", "cold_collect_stderr 2 20");
    for (case, expected_status, collection_status, wait_status, timed_out, reaped) in [
        ("normal", 0, 0, "0", false, true),
        ("tracer_failure", 23, 0, "0", false, true),
        ("collector_failure", 1, 1, "1", false, true),
        ("held_pipe", 1, 124, "137", true, true),
        ("wrong_start_ticks", 1, 125, "not_waited", false, false),
        ("wrong_parent", 1, 125, "not_waited", false, false),
    ] {
        let proof = tempfile::tempdir().unwrap();
        let environment = BTreeMap::from([
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("COLD_CASE".into(), case.into()),
            ("COLD_PROOF".into(), proof.path().to_str().unwrap().into()),
        ]);
        let harness = format!(
            r#"set -u
umask 077
set -C
cold_parent=$BASHPID
cold_proof=$COLD_PROOF
cold_started=unit-test
cold_collector_current() {{
{functions}
cold_launch() {{
exec {{cold_stderr_fd}}> >(
{launch}
}}
if [[ $COLD_CASE == collector_failure ]]; then cold_launch 2>/dev/full; else cold_launch; fi
for ((poll=0; poll<100; poll++)); do
  [[ ! -s $cold_proof/cold-stderr.start.stat ]] || break
  /usr/bin/sleep 0.01
done
cold_status=0
if [[ $COLD_CASE == tracer_failure ]]; then cold_status=23; fi
printf 'stderr payload\n' >&"$cold_stderr_fd"
cold_writer=
if [[ $COLD_CASE == held_pipe || $COLD_CASE == wrong_* ]]; then
  /usr/bin/mkfifo "$cold_proof/writer-release"
  exec {{cold_release_fd}}<> "$cold_proof/writer-release"
  (
    IFS= read -r writer_row < "/proc/$BASHPID/stat"
    printf '%s\n' "$writer_row" > "$cold_proof/writer-ready"
    IFS= read -r -t 3 release <&"$cold_release_fd"
    test "$release" = release
  ) 2>&"$cold_stderr_fd" &
  cold_writer=$!
  for ((poll=0; poll<100; poll++)); do
    [[ ! -s $cold_proof/writer-ready ]] || break
    /usr/bin/sleep 0.01
  done
  IFS= read -r writer_row < "$cold_proof/writer-ready"
  [[ $writer_row == "$cold_writer "* ]]
  tail=${{writer_row##*) }}
  IFS=' ' read -r -a writer_fields <<< "$tail"
  [[ ${{writer_fields[1]}} == "$cold_parent" && ${{writer_fields[19]}} =~ ^[1-9][0-9]*$ ]]
fi
if [[ $COLD_CASE == wrong_* ]]; then
  IFS= read -r real_row < "$cold_proof/cold-stderr.start.stat"
  tail=${{real_row##*) }}
  IFS=' ' read -r -a fields <<< "$tail"
  real_ticks=${{fields[19]}}
  if [[ $COLD_CASE == wrong_parent ]]; then fields[1]=0; else fields[19]=$((real_ticks+1)); fi
  printf '%s (negative) %s\n' "$cold_stderr_pid" "${{fields[*]}}" >| "$cold_proof/cold-stderr.start.stat"
fi
for record in metadata sha256 version version.stderr; do
  printf 'unit-provider\n' > "$cold_proof/cold-strace.before.$record"
done
cold_provider() {{
  test "$1" = after && test -s "$cold_proof/cold-tracer.status" || return 1
  printf 'after\n' > "$cold_proof/provider-after" || return 1
  for record in metadata sha256 version version.stderr; do
    printf 'unit-provider\n' > "$cold_proof/cold-strace.after.$record" || return 1
  done
}}
cold_finish() {{
cold_records_status=0
{finish}
}}
result=0
cold_finish || result=$?
if [[ $COLD_CASE == wrong_* ]]; then
  cold_stderr_ticks=$real_ticks
  cold_collector_current
  printf '%s\n' "$real_row" >| "$cold_proof/cold-stderr.start.stat"
fi
if [[ -n $cold_writer ]]; then
  printf 'release\n' >&"$cold_release_fd"
  exec {{cold_release_fd}}>&-
  # Writer has its own 3s read deadline; NativeRunner owns the outer 5s cleanup.
  wait "$cold_writer"
fi
if [[ $COLD_CASE == wrong_* ]]; then
  cold_collect_stderr 2 20
  test "$cold_stderr_wait_status" = 0 || exit 91
  test "$cold_stderr_reaped" = true || exit 92
fi
printf 'postconditions-verified\n' > "$cold_proof/collector-control-complete"
exit "$result"
"#
        );
        let status = run_shell(&harness, &environment).unwrap();
        assert_eq!(status.code(), Some(expected_status), "{case}");
        if case != "tracer_failure" {
            assert_eq!(
                fs::read_to_string(proof.path().join("collector-control-complete")).unwrap(),
                "postconditions-verified\n",
                "{case}: a failed cleanup assertion must not count as expected exit 1"
            );
        }
        let tracer_status = if case == "tracer_failure" { 23 } else { 0 };
        assert_eq!(
            fs::read_to_string(proof.path().join("cold-tracer.status")).unwrap(),
            format!("shell_wait_status={tracer_status}\n"),
            "{case}"
        );
        let receipt = fs::read_to_string(proof.path().join("cold-tracer.result")).unwrap();
        for line in [
            format!("shell_wait_status={tracer_status}"),
            format!("stderr_collection_status={collection_status}"),
            format!("stderr_wait_status={wait_status}"),
            format!("stderr_timed_out={timed_out}"),
            format!("stderr_reaped={reaped}"),
            "strace_debug_requested=true".into(),
            "diagnostic_only=true".into(),
            "stderr_file_limit_bytes=536870912".into(),
            "seccomp_bpf_activation=unproven".into(),
        ] {
            assert!(
                receipt.lines().any(|actual| actual == line),
                "{case}: {line}"
            );
        }
        let stderr_bytes = receipt
            .lines()
            .find_map(|line| line.strip_prefix("stderr_bytes_observed="))
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!(stderr_bytes <= 512 * 1024 * 1024, "{case}");
        if reaped {
            assert_eq!(
                stderr_bytes,
                fs::metadata(proof.path().join("prepared-cold-bootstrap.stderr"))
                    .unwrap()
                    .len(),
                "{case}"
            );
        }
        assert_eq!(
            fs::read_to_string(proof.path().join("provider-after")).unwrap(),
            "after\n",
            "{case}: provider-after must run even when collection fails"
        );
    }
}

#[test]
fn future_diagnostic_runner_and_fork_refusal_require_static_approved_routing() {
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    validate_local_routing(&workflow).unwrap();
    for runner in [
        serde_json::json!(LOCAL_RUNNER),
        serde_json::json!(HOSTED_RUNNER),
    ] {
        let mut approved = workflow.clone();
        approved["jobs"]["native"]["runs-on"] = runner;
        validate_local_routing(&approved).unwrap();
    }
    for runner in [
        serde_json::json!("ubuntu-latest"),
        serde_json::json!("windows-latest"),
        serde_json::json!("ubuntu-24.04-arm"),
        serde_json::json!(["ubuntu-24.04"]),
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
            ("NATIVE_TOOLS".into(), "success".into()),
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

fn current_common_file_sha<'a>(
    contract: &'a serde_json::Value,
    name: &str,
    historical: &'a serde_json::Value,
) -> &'a str {
    if name != "tools/ci/local-arm" {
        return historical["sha256"].as_str().unwrap();
    }
    let overlay = &contract["github_hosted_535_overlay"];
    assert_eq!(overlay["issue"], 535);
    assert_eq!(
        overlay["baseline_source_commit"],
        "77663836a95a9e2aff9aa66592d8b06ed9c87cd9"
    );
    assert_eq!(overlay["files"].as_object().unwrap().len(), 1);
    assert_eq!(overlay["historical_acceptance_changed"], false);
    let current = &overlay["files"][name];
    assert_eq!(current["preimage_sha256"], historical["sha256"]);
    assert_eq!(
        current["sha256"],
        "a286863a38d2d122bd97b7f648086c1763e3aa863fe36fe7101f2f7a6ae7ada0"
    );
    current["sha256"].as_str().unwrap()
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
    assert_eq!(
        contract["action_distribution"]["commit"],
        "b2fc62472ce8183114d59dbeec4d1406df44e4c2"
    );
    assert_eq!(
        contract["action_distribution"]["source_commit"],
        contract["source_commit"]
    );
    for (name, source) in inputs() {
        let workflow = parse(&source).unwrap();
        for job in workflow["jobs"].as_object().unwrap().values() {
            for step in job["steps"].as_array().into_iter().flatten() {
                let Some(reference) = step["uses"].as_str() else {
                    continue;
                };
                for action in ["job-temp", "rust-path", "local-arm"] {
                    let prefix = format!("bin9208/openpilot-rust/.github/actions/{action}@");
                    if reference.starts_with(&prefix) {
                        assert_eq!(
                            reference,
                            format!("{prefix}b2fc62472ce8183114d59dbeec4d1406df44e4c2"),
                            "common distribution pin drift: {name}"
                        );
                    }
                }
            }
        }
    }
    for (name, seal) in contract["files"].as_object().unwrap() {
        let bytes = fs::read(root().join(name)).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            current_common_file_sha(&contract, name, seal),
            "common interface drift: {name}"
        );
    }
    let source = fs::read_to_string(root().join("tools/ci/local-arm")).unwrap();
    assert!(source.contains("physical_arm_cpu\":false"));
    assert!(source.contains("[[ $guest_machine == aarch64 ]]"));
    assert!(source.contains("[[ $(git -C \"$source_dir\" rev-parse HEAD) == \"$GITHUB_SHA\" ]]"));
}

#[test]
fn integration_aggregate_cannot_pass_failed_or_skipped_workers_with_or_true() {
    let mut files = inputs();
    let source = &mut files
        .iter_mut()
        .find(|(name, _)| name == "integration.yml")
        .unwrap()
        .1;
    assert!(source.contains("test \"$JETLINK\" = success"));
    *source = source.replacen(
        "test \"$JETLINK\" = success",
        "test \"$JETLINK\" = success || true",
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
        assert_eq!(worker["runs-on"], serde_json::json!(HOSTED_RUNNER));
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
        let mut legacy = workflow.clone();
        legacy["jobs"][name]["runs-on"] = serde_json::json!(LOCAL_RUNNER);
        validate_fast_gate(&legacy).unwrap();
        for runner in ["ubuntu-latest", "windows-latest", "${{ matrix.runner }}"] {
            let mut unapproved = workflow.clone();
            unapproved["jobs"][name]["runs-on"] = runner.into();
            assert!(validate_fast_gate(&unapproved).is_err());
        }
        assert_eq!(steps[0]["env"]["SOURCE_ALLOWED"], DIAGNOSTIC_TRUSTED_SOURCE);
    }
    let mut fork_pass = workflow.clone();
    fork_pass["jobs"]["fast"]["steps"][0]["env"]["SOURCE_ALLOWED"] = "true".into();
    assert!(validate_fast_gate(&fork_pass).is_err());
}

#[test]
fn native_fast_after_guards_reject_skipped_audits_and_unsealed_cleanup() {
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    validate_local_routing(&workflow).unwrap();
    validate_fast_gate(&workflow).unwrap();
    let steps = workflow["jobs"]["native"]["steps"].as_array().unwrap();
    let at = |id: &str| steps.iter().position(|step| step["id"] == id).unwrap();
    assert!(at("prepared_rust") < at("sdk_permissions"));
    let mut late_prepare = workflow.clone();
    late_prepare["jobs"]["native"]["steps"]
        .as_array_mut()
        .unwrap()
        .swap(at("prepared_rust"), at("sdk_permissions"));
    assert!(validate_fast_gate(&late_prepare).is_err());

    // after 검사는 실패 때도 실행해야 하며 fork/source 경계 밖으로 빠져나갈 수 없다.
    for id in [
        "sdk_permissions_final",
        "prepared_rust_final",
        "source_final",
    ] {
        let index = at(id);
        let condition = steps[index]["if"].as_str().unwrap();
        for changed_condition in [
            TRUSTED_SOURCE.to_owned(),
            "${{ always() }}".to_owned(),
            condition.replace("always()", "success()"),
            condition.replace(" }}", " || true }}"),
            condition.replace(" }}", " && false }}"),
        ] {
            let mut changed = workflow.clone();
            changed["jobs"]["native"]["steps"][index]["if"] = changed_condition.into();
            assert!(
                validate_fast_gate(&changed).is_err(),
                "accepted {id} guard mutation"
            );
        }
        let mut changed = workflow.clone();
        changed["jobs"]["native"]["steps"][index]["if"] = "${{ always() }}".into();
        assert!(validate_local_routing(&changed).is_err());
    }
    for (id, clause) in [
        (
            "sdk_permissions_final",
            " && steps.sdk_permissions.outcome != 'skipped'",
        ),
        ("sdk_permissions_final", " && env.DIAG_SOURCE != ''"),
        (
            "prepared_rust_final",
            " && steps.prepared_rust.outcome == 'success'",
        ),
        ("source_final", " && env.DIAG_SOURCE != ''"),
    ] {
        let index = at(id);
        let condition = steps[index]["if"].as_str().unwrap();
        assert!(condition.contains(clause));
        let mut changed = workflow.clone();
        changed["jobs"]["native"]["steps"][index]["if"] = condition.replace(clause, "").into();
        assert!(
            validate_fast_gate(&changed).is_err(),
            "accepted missing {clause}"
        );
    }
    let cleanup = at("fast_source_cleanup");
    let condition = steps[cleanup]["if"].as_str().unwrap();
    for clause in [
        " && steps.fast_evidence.outcome == 'success'",
        " && steps.fast_evidence.outputs.artifact-id != ''",
        " && steps.fast_evidence.outputs.artifact-digest != ''",
        " && env.DIAG_SOURCE_OWNER != ''",
        " && steps.source_final.outcome == 'success'",
        " && steps.prepared_rust_final.outcome == 'success'",
        " && steps.sdk_permissions_final.outcome == 'success'",
    ] {
        assert!(condition.contains(clause));
        let mut changed = workflow.clone();
        changed["jobs"]["native"]["steps"][cleanup]["if"] = condition.replace(clause, "").into();
        assert!(
            validate_fast_gate(&changed).is_err(),
            "accepted unsealed cleanup: {clause}"
        );
    }
    for id in [
        "sdk_permissions",
        "sdk_permissions_final",
        "prepared_rust",
        "prepared_rust_final",
        "source_final",
        "fast_evidence",
        "fast_source_cleanup",
    ] {
        let mut changed = workflow.clone();
        changed["jobs"]["native"]["steps"][at(id)]["id"] = "unbound_outcome".into();
        assert!(
            validate_fast_gate(&changed).is_err(),
            "accepted unbound {id}"
        );
        let mut changed = workflow.clone();
        changed["jobs"]["native"]["steps"]
            .as_array_mut()
            .unwrap()
            .push(steps[at(id)].clone());
        assert!(
            validate_fast_gate(&changed).is_err(),
            "accepted duplicated {id}"
        );
    }
}

#[test]
fn hosted_source_boundary_names_and_denials_do_not_become_aggregate_bypasses() {
    use openpilot_dev_tools::ci_policy::FORK_DENIAL;
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    for (name, denial) in [
        ("Deny fork source and require GitHub-hosted Linux X64", "set -euo pipefail\ntest \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied for trusted docs validation' >&2; exit 1; }\n"),
        ("Deny fork source and require GitHub-hosted Ubuntu X64 runner", "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied for setup qualification' >&2; exit 1; }\n"),
        ("Deny fork source and require GitHub-hosted Ubuntu X64 runner", "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied for normal40 qualification' >&2; exit 1; }\n"),
    ] {
        let mut hosted = workflow.clone();
        let boundary = &mut hosted["jobs"]["native"]["steps"][0];
        boundary["name"] = name.into();
        boundary["run"] = boundary["run"].as_str().unwrap().replacen(FORK_DENIAL, denial, 1).into();
        validate_local_routing(&hosted).unwrap();
        let mut bypass = hosted.clone();
        bypass["jobs"]["native"]["steps"][1]["if"] = "${{ always() }}".into();
        assert!(validate_local_routing(&bypass).is_err());
        let mut skipped = hosted.clone();
        skipped["jobs"]["native"]["steps"][0]["if"] = "${{ always() }}".into();
        assert!(validate_local_routing(&skipped).is_err());
        let mut fail_open = hosted.clone();
        fail_open["jobs"]["native"]["steps"][0]["run"] = denial.replace("exit 1", "exit 0").into();
        assert!(validate_local_routing(&fail_open).is_err());
    }
    let mut fake_aggregate = workflow.clone();
    fake_aggregate["jobs"]["native"]["if"] = "${{ always() }}".into();
    fake_aggregate["jobs"]["native"]["steps"][0]["name"] = "unapproved source boundary".into();
    assert!(validate_local_routing(&fake_aggregate).is_err());
}

#[test]
fn all_fast_workers_require_bound_sdk_seals_and_sealed_cleanup() {
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    validate_fast_gate(&workflow).unwrap();
    for worker in [
        "native",
        "native-tools",
        "native-corpus",
        "native-alerts",
        "native-alert-manager",
        "native-timing",
        "python-android",
    ] {
        let steps = workflow["jobs"][worker]["steps"].as_array().unwrap();
        let at = |id: &str| steps.iter().position(|s| s["id"] == id).unwrap();
        for id in [
            "prepared_rust",
            "prepared_rust_final",
            "source_final",
            "fast_evidence",
            "fast_source_cleanup",
        ] {
            let index = at(id);
            let mut missing = workflow.clone();
            missing["jobs"][worker]["steps"]
                .as_array_mut()
                .unwrap()
                .remove(index);
            assert!(
                validate_fast_gate(&missing).is_err(),
                "{worker}: missing {id}"
            );
            let mut duplicate = workflow.clone();
            duplicate["jobs"][worker]["steps"]
                .as_array_mut()
                .unwrap()
                .push(steps[index].clone());
            assert!(
                validate_fast_gate(&duplicate).is_err(),
                "{worker}: duplicate {id}"
            );
            let mut unbound = workflow.clone();
            unbound["jobs"][worker]["steps"][index]["id"] = "unbound_outcome".into();
            assert!(
                validate_fast_gate(&unbound).is_err(),
                "{worker}: unbound {id}"
            );
            let mut skipped = workflow.clone();
            skipped["jobs"][worker]["steps"][index]["if"] = "false".into();
            assert!(
                validate_fast_gate(&skipped).is_err(),
                "{worker}: skipped {id}"
            );
        }
        for id in ["prepared_rust_final", "source_final", "fast_evidence"] {
            let index = at(id);
            let condition = steps[index]["if"].as_str().unwrap();
            for guard in [
                TRUSTED_SOURCE.to_owned(),
                "${{ always() }}".to_owned(),
                condition.replace("always()", "success()"),
                condition.replace(" }}", " && false }}"),
            ] {
                let mut skipped = workflow.clone();
                skipped["jobs"][worker]["steps"][index]["if"] = guard.into();
                assert!(
                    validate_fast_gate(&skipped).is_err(),
                    "{worker}: skip or untrusted {id}"
                );
            }
        }
        let cleanup = at("fast_source_cleanup");
        let condition = steps[cleanup]["if"].as_str().unwrap();
        for clause in [
            " && steps.fast_evidence.outcome == 'success'",
            " && steps.fast_evidence.outputs.artifact-id != ''",
            " && steps.fast_evidence.outputs.artifact-digest != ''",
            " && env.DIAG_SOURCE_OWNER != ''",
            " && steps.source_final.outcome == 'success'",
            " && steps.prepared_rust_final.outcome == 'success'",
        ] {
            assert!(condition.contains(clause));
            let mut unsafe_cleanup = workflow.clone();
            unsafe_cleanup["jobs"][worker]["steps"][cleanup]["if"] =
                condition.replace(clause, "").into();
            assert!(
                validate_fast_gate(&unsafe_cleanup).is_err(),
                "{worker}: cleanup without {clause}"
            );
        }
        let mut premature = workflow.clone();
        premature["jobs"][worker]["steps"]
            .as_array_mut()
            .unwrap()
            .swap(at("prepared_rust"), at("prepared_rust_final"));
        assert!(
            validate_fast_gate(&premature).is_err(),
            "{worker}: final seal before checks"
        );
    }
}

#[test]
fn fast_sdk_opt_in_stays_inline_and_readonly_helpers_cannot_be_skipped() {
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    validate_fast_gate(&workflow).unwrap();
    let variables = [
        "OPENPILOT_CI_PREPARED_RUST_ROOT",
        "OPENPILOT_CI_PREPARED_RUST_MANIFEST",
    ];
    for variable in variables {
        assert!(workflow["env"].get(variable).is_none());
        let mut global = workflow.clone();
        global["env"] = serde_json::json!({});
        global["env"][variable] = "/unexpected/global/opt-in".into();
        assert!(validate_fast_gate(&global).is_err());
    }
    for (worker, job) in workflow["jobs"].as_object().unwrap() {
        if worker == "fast" {
            continue;
        }
        for variable in variables {
            assert!(job["env"].get(variable).is_none());
            let mut global = workflow.clone();
            global["jobs"][worker]["env"][variable] = "/unexpected/job/opt-in".into();
            assert!(validate_fast_gate(&global).is_err());
        }
        let steps = job["steps"].as_array().unwrap();
        for (index, step) in steps.iter().enumerate() {
            for variable in variables {
                assert!(step["env"].get(variable).is_none());
            }
            let body = step["run"].as_str().unwrap_or("");
            // Mock bootstrap tests retain the unprepared API: no global opt-in export.
            assert!(!body.contains("export OPENPILOT_CI_PREPARED_"));
            assert!(!body.lines().any(|line| {
                line.contains("OPENPILOT_CI_PREPARED_") && line.contains("GITHUB_ENV")
            }));
            if body.contains("/rust/tools/check-disk-space\"") {
                for variable in variables {
                    let mut inherited = workflow.clone();
                    inherited["jobs"][worker]["steps"][index]["env"][variable] =
                        "/unexpected/step/opt-in".into();
                    assert!(validate_fast_gate(&inherited).is_err());
                }
                for binding in [
                    "OPENPILOT_CI_PREPARED_RUST_ROOT=\"$CI_PREPARED_RUST_ROOT\" ",
                    "OPENPILOT_CI_PREPARED_RUST_MANIFEST=\"$CI_PREPARED_RUST_MANIFEST\" ",
                ] {
                    assert!(body.contains(binding));
                    let mut unprepared = workflow.clone();
                    unprepared["jobs"][worker]["steps"][index]["run"] =
                        body.replace(binding, "").into();
                    assert!(
                        validate_fast_gate(&unprepared).is_err(),
                        "{worker}: missing inline {binding}"
                    );
                }
            }
            if ["prepared_rust", "prepared_rust_final"]
                .iter()
                .any(|id| step["id"] == *id)
            {
                // Admission/final must call their distinct bound helper phases.
                let wrong_phase = if step["id"] == "prepared_rust" {
                    body.replace(" before", " after")
                } else {
                    body.replace(" after", " before")
                };
                for replacement in ["true", wrong_phase.as_str()] {
                    let mut bypass = workflow.clone();
                    bypass["jobs"][worker]["steps"][index]["run"] = replacement.into();
                    assert!(
                        validate_fast_gate(&bypass).is_err(),
                        "{worker}: helper bypass"
                    );
                }
                if worker != "native" {
                    assert!(!steps.iter().any(|s| {
                        s["id"] == "sdk_permissions" || s["id"] == "sdk_permissions_final"
                    }));
                }
            }
        }
    }
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
    assert_eq!(
        contract["action_distribution"]["commit"],
        "b2fc62472ce8183114d59dbeec4d1406df44e4c2"
    );
    assert_eq!(
        contract["action_distribution"]["source_commit"],
        contract["common_source"]
    );
    for (name, seal) in contract["common_files"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root().join(name)).unwrap())),
            current_common_file_sha(&contract, name, seal),
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
    assert_eq!(covered, 61, "maintained private source roster changed");
}

#[test]
fn darwin_uses_actual_hosted_arm64_without_linux_or_generic_runner_fallback() {
    use openpilot_dev_tools::ci_policy::DARWIN_RUNNER;
    let original =
        parse(&fs::read_to_string(root().join(".github/workflows/rust-dbc-darwin.yml")).unwrap())
            .unwrap();
    assert_eq!(original["jobs"]["dbc-darwin"]["runs-on"], DARWIN_RUNNER);
    validate_local_routing(&original).unwrap();
    let mut changed = original.clone();
    changed["jobs"]["dbc-darwin"]["runs-on"] = HOSTED_RUNNER.into();
    assert!(
        validate_local_routing(&changed).is_err(),
        "Linux cannot qualify Darwin"
    );
    let mut changed = original.clone();
    changed["jobs"]["dbc-darwin"]["env"]["CI296_ALIAS_BASENAME"] = ".ci296-source-foreign".into();
    assert!(validate_local_routing(&changed).is_err());
    for marker in [
        "test \"$GITHUB_ACTIONS\" = true",
        "test \"$GITHUB_REPOSITORY\" = bin9208/openpilot-rust",
        "test \"$RUNNER_ENVIRONMENT\" = github-hosted",
        "test \"$RUNNER_OS\" = macOS",
        "test \"$RUNNER_ARCH\" = ARM64",
        "test \"$(uname -s)\" = Darwin",
        "test \"$(uname -m)\" = arm64",
    ] {
        let mut changed = original.clone();
        let body = original["jobs"]["dbc-darwin"]["steps"][0]["run"]
            .as_str()
            .unwrap();
        assert!(body.contains(marker));
        changed["jobs"]["dbc-darwin"]["steps"][0]["run"] = body.replace(marker, "true").into();
        assert!(
            validate_local_routing(&changed).is_err(),
            "missing native boundary: {marker}"
        );
    }
    let mut commented = original.clone();
    let boundary = original["jobs"]["dbc-darwin"]["steps"][0]["run"]
        .as_str()
        .unwrap();
    commented["jobs"]["dbc-darwin"]["steps"][0]["run"] = boundary
        .replace(
            "test \"$RUNNER_OS\" = macOS",
            "# test \"$RUNNER_OS\" = macOS",
        )
        .into();
    assert!(
        validate_local_routing(&commented).is_err(),
        "a comment cannot substitute for physical guards"
    );
    let mut fast =
        parse(&fs::read_to_string(root().join(".github/workflows/fast-checks.yml")).unwrap())
            .unwrap();
    fast["jobs"]["native"]["runs-on"] = DARWIN_RUNNER.into();
    assert!(
        validate_local_routing(&fast).is_err(),
        "Darwin admission must be limited to its owned job"
    );
}

#[test]
fn docs_private_cleanup_requires_exact_sdk_and_owned_recovery_receipts() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/user-docs.yaml")).unwrap())
            .unwrap();
    let job = &workflow["jobs"]["check_user_docs"];
    validate_private_job_layout(job).unwrap();
    let steps = job["steps"].as_array().unwrap();
    let at = |id: &str| steps.iter().position(|step| step["id"] == id).unwrap();
    let cleanup = at("ci296_source_cleanup");
    for id in ["prepared_rust_final", "ci296_docs_recovery"] {
        let mut changed = job.clone();
        let original = steps[cleanup]["if"].as_str().unwrap();
        changed["steps"][cleanup]["if"] = original
            .replace(&format!(" && steps.{id}.outcome == 'success'"), "")
            .into();
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "docs cleanup omitted {id}"
        );
        let mut changed = job.clone();
        changed["steps"][at(id)]["run"] = "true".into();
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "docs safety body omitted {id}"
        );
        let mut changed = job.clone();
        changed["steps"][at(id)]["if"] = TRUSTED_SOURCE.into();
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "docs finalizer skipped on failure: {id}"
        );
    }
    for name in [
        "Check code and user documentation changes",
        "Test user documentation checker",
    ] {
        let mut changed = job.clone();
        let command = steps.iter().position(|step| step["name"] == name).unwrap();
        changed["steps"]
            .as_array_mut()
            .unwrap()
            .swap(command, at("ci296_source_seal"));
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "docs command must precede its source seal"
        );
    }
    let mut changed = job.clone();
    changed["steps"][at("ci296_docs_recovery")]["run"] = steps[at("ci296_docs_recovery")]["run"]
        .as_str()
        .unwrap()
        .replace(
            "owner=$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB",
            "owner=foreign",
        )
        .into();
    assert!(validate_private_job_layout(&changed).is_err());
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
                let finish = at(if job["env"]["CI296_NATIVE_ARM"] == "1" {
                    "Finish native ARM execution evidence"
                } else {
                    "Finish emulated architecture evidence"
                });
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
    assert_eq!((sources, arm), (61, 9));
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
// #514: always-run uploads/seals do not establish native WebRTC success.
const WEBRTC_REQUIRED_OUTCOMES: [&str; 4] = [
    "webrtc_dependency_stager",
    "webrtc_original_bindings",
    "webrtc_native_checks",
    "webrtc_real_comparisons",
];

fn webrtc_retention_job() -> serde_json::Value {
    parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap()["jobs"]
        ["webrtc-runtime"]
        .clone()
}

fn webrtc_cleanup_condition_passes(
    condition: &str,
    outcomes: &BTreeMap<String, String>,
    cancelled: bool,
    successful: bool,
) -> bool {
    let condition = condition
        .replace("!cancelled()", if cancelled { "1 == 0" } else { "1 == 1" })
        .replace("success()", if successful { "1 == 1" } else { "1 == 0" });
    let actual = ownership_condition_as_bash(&condition, outcomes);
    run_shell(
        &format!("if [[ {actual} ]]; then exit 0; else exit 17; fi"),
        &BTreeMap::new(),
    )
    .unwrap()
    .success()
}

#[test]
fn webrtc_required_native_failures_and_cancellation_retain_both_cleanup_roots() {
    let job = webrtc_retention_job();
    let steps = job["steps"].as_array().unwrap();
    let success: BTreeMap<String, String> = WEBRTC_REQUIRED_OUTCOMES
        .iter()
        .map(|id| ((*id).into(), "success".into()))
        .collect();
    for cleanup in [
        "Clean only uploaded owned source alias and target",
        "Clean common owned temporary paths after uploaded source",
    ] {
        let condition = steps.iter().find(|s| s["name"] == cleanup).unwrap()["if"]
            .as_str()
            .unwrap();
        assert!(webrtc_cleanup_condition_passes(
            condition, &success, false, true
        ));
        // Keep receipts and native outcomes successful to isolate each status
        // guard, including cancellation after the native stages have completed.
        assert!(!webrtc_cleanup_condition_passes(
            condition, &success, true, true
        ));
        assert!(!webrtc_cleanup_condition_passes(
            condition, &success, false, false
        ));
        for id in WEBRTC_REQUIRED_OUTCOMES {
            for outcome in ["failure", "cancelled", "skipped", ""] {
                let mut outcomes = success.clone();
                outcomes.insert(id.into(), outcome.into());
                // Also hold success() true to prove the explicit outcome guard
                // independently rejects a failed/skipped producer or comparison.
                for successful in [false, true] {
                    assert!(
                        !webrtc_cleanup_condition_passes(condition, &outcomes, false, successful),
                        "{cleanup}: {id}={outcome:?} deleted failed evidence"
                    );
                }
            }
        }
    }
}

#[test]
fn webrtc_retention_policy_rejects_guard_removal_and_unbound_required_stages() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let job = webrtc_retention_job();
    validate_private_job_layout(&job).unwrap();
    let steps = job["steps"].as_array().unwrap();
    for cleanup in [
        "Clean only uploaded owned source alias and target",
        "Clean common owned temporary paths after uploaded source",
    ] {
        let index = steps.iter().position(|s| s["name"] == cleanup).unwrap();
        let original = steps[index]["if"].as_str().unwrap();
        for clause in ["!cancelled()".to_owned(), "success()".to_owned()]
            .into_iter()
            .chain(WEBRTC_REQUIRED_OUTCOMES.map(|id| format!("steps.{id}.outcome == 'success'")))
        {
            let mut changed = job.clone();
            let condition = original.replacen(&format!(" && {clause}"), "", 1);
            assert_ne!(condition, original, "missing mutation anchor: {clause}");
            changed["steps"][index]["if"] = condition.into();
            assert!(
                validate_private_job_layout(&changed).is_err(),
                "{cleanup}: guard removed: {clause}"
            );
        }
        let mut changed = job.clone();
        changed["steps"][index]["if"] = original.replace(".outcome", ".conclusion").into();
        assert!(validate_private_job_layout(&changed).is_err());
    }
    for id in WEBRTC_REQUIRED_OUTCOMES {
        let index = steps.iter().position(|s| s["id"] == id).unwrap();
        let mut changed = job.clone();
        changed["steps"][index]
            .as_object_mut()
            .unwrap()
            .remove("id");
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "missing {id}"
        );
        let mut changed = job.clone();
        changed["steps"][index]["continue-on-error"] = "true".into();
        assert!(validate_private_job_layout(&changed).is_err(), "COE {id}");
        let mut changed = job.clone();
        changed["steps"][index]["if"] = "${{ always() }}".into();
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "not normal {id}"
        );
        let mut changed = job.clone();
        changed["steps"][0]["id"] = id.into();
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "duplicate {id}"
        );
    }
}
#[test]
fn flow_mapping_alias_keys_keep_an_explicit_colon_separator() {
    use openpilot_dev_tools::ci_policy::parse;
    let source = "{keys: [&key run, &body 'cargo test'], steps: [{*key : *body}]}";
    let value = parse(source).unwrap();
    assert_eq!(value["steps"][0]["run"], "cargo test");
    assert!(parse("{keys: [&key run], steps: [{*unknown : value}]}").is_err());
}

#[test]
fn hosted_arm_requires_explicit_intent_and_actual_runner_guards() {
    let original = parse(
        &fs::read_to_string(root().join(".github/workflows/python-rust-523-hosted-arm.yml"))
            .unwrap(),
    )
    .unwrap();
    validate_local_routing(&original).unwrap();
    for key in ["CI296_NATIVE_ARM", "SOURCE_ALLOWED"] {
        let mut changed = original.clone();
        let env = if key == "CI296_NATIVE_ARM" {
            &mut changed["jobs"]["inventory"]["env"]
        } else {
            &mut changed["jobs"]["inventory"]["steps"][0]["env"]
        };
        env.as_object_mut().unwrap().remove(key);
        assert!(validate_local_routing(&changed).is_err(), "missing {key}");
    }
    for guard in [
        "test \"$RUNNER_ENVIRONMENT\" = github-hosted\n",
        "test \"$RUNNER_ARCH\" = ARM64\n",
        "test \"$(uname -m)\" = aarch64\n",
        "test \"$GITHUB_REF\" = refs/heads/codex/feat-523-python-rust\n",
    ] {
        let mut changed = original.clone();
        let body = changed["jobs"]["inventory"]["steps"][0]["run"]
            .as_str()
            .unwrap();
        assert_eq!(body.matches(guard).count(), 1);
        changed["jobs"]["inventory"]["steps"][0]["run"] = body.replace(guard, "").into();
        assert!(validate_local_routing(&changed).is_err(), "missing {guard}");
    }
    let mut changed = original.clone();
    changed["jobs"]["inventory"]["runs-on"] = "${{ matrix.runner }}".into();
    assert!(validate_local_routing(&changed).is_err());
}

#[test]
fn generated_asset_cleanup_requires_completed_real_consumers() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/tests.yaml")).unwrap()).unwrap();
    let job = &workflow["jobs"]["build_release"];
    validate_private_job_layout(job).unwrap();
    let steps = job["steps"].as_array().unwrap();
    let generated = steps
        .iter()
        .position(|s| s["id"] == "generated_assets_check")
        .unwrap();
    let cleanup = steps
        .iter()
        .position(|s| s["id"] == "ci296_source_cleanup")
        .unwrap();
    let mut changed = job.clone();
    changed["steps"][cleanup]["if"] = steps[cleanup]["if"]
        .as_str()
        .unwrap()
        .replace(" && steps.generated_assets_check.outcome == 'success'", "")
        .into();
    assert!(validate_private_job_layout(&changed).is_err());
    for mode in ["renamed", "continue", "always"] {
        let mut changed = job.clone();
        match mode {
            "renamed" => changed["steps"][generated]["id"] = "unbound_assets".into(),
            "continue" => changed["steps"][generated]["continue-on-error"] = "true".into(),
            _ => changed["steps"][generated]["if"] = "${{ always() }}".into(),
        }
        assert!(validate_private_job_layout(&changed).is_err(), "{mode}");
    }
}

#[test]
fn native_arm_transport_rejects_unbound_or_shared_shell() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflow = parse(
        &fs::read_to_string(root().join(".github/workflows/python-rust-523-native-arm.yml"))
            .unwrap(),
    )
    .unwrap();
    validate_local_routing(&workflow).unwrap();
    let job = &workflow["jobs"]["carrot-man-runtime"];
    validate_private_job_layout(job).unwrap();
    let prepare = job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .position(|s| s["name"] == "Prepare native ARM command transport")
        .unwrap();
    for body in ["true\n", "/usr/bin/bash tools/ci/native-arm prepare\n"] {
        let mut changed = job.clone();
        changed["steps"][prepare]["run"] = body.into();
        assert!(validate_private_job_layout(&changed).is_err());
    }
}
