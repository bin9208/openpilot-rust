use openpilot_dev_tools::{
    planner_ci_controller::{
        plan::{self, Inputs, CHECK_NAMES, MEMORY_CHECK_NAMES},
        run_checks, scheduler, Check, CheckFailure,
    },
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

fn endpoint(cases: &[[f64; 3]]) -> Vec<f64> {
    let request = cases
        .iter()
        .map(|[nominal, sent, done]| format!("{nominal:?}\t{sent:?}\t{done:?}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    scheduler::serve(Cursor::new(request), &mut output).unwrap();
    std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|value| value.parse().unwrap())
        .collect()
}

#[test]
fn ipc_replay_keeps_normal_frequency_independent_of_solver_duration() {
    let cases = [0.003, 0.013, 0.025, 0.049].map(|work| [100.05, 100.0, 100.0 + work]);
    for target in endpoint(&cases) {
        assert!((target - 100.05).abs() <= 1e-12);
    }
}

#[test]
fn ipc_replay_does_not_catch_up_after_delayed_model_processing() {
    let actual = endpoint(&[[100.05, 100.0, 100.08], [100.05, 100.04, 100.045]]);
    for (actual, expected) in actual.into_iter().zip([100.13, 100.09]) {
        assert!((actual - expected).abs() <= 1e-12);
    }
}

fn signed_zero_cases() -> [[f64; 3]; 4] {
    [
        [-0.0, -0.05, -0.05],
        [0.0, -0.05, -0.05],
        [-0.0, -0.1, -0.05],
        [0.0, -0.1, -0.05],
    ]
}

#[test]
fn scheduler_keeps_the_first_signed_zero_at_both_python_max_ties() {
    let cases = signed_zero_cases();
    for (case, target) in cases.iter().zip(endpoint(&cases)) {
        assert_eq!(target.to_bits(), case[0].to_bits());
        assert_eq!(
            scheduler::next_model_target(case[0], case[1], case[2]).to_bits(),
            case[0].to_bits()
        );
    }
}

#[test]
fn public_scheduler_keeps_python_unordered_nan_and_infinity_semantics() {
    let nan = f64::from_bits(0x7ff8_0000_0000_0042);
    assert_eq!(
        scheduler::next_model_target(nan, 0.0, 0.08).to_bits(),
        nan.to_bits()
    );
    assert_eq!(
        scheduler::next_model_target(100.05, nan, 100.08).to_bits(),
        100.05_f64.to_bits()
    );
    assert_eq!(
        scheduler::next_model_target(100.05, 100.0, nan).to_bits(),
        100.05_f64.to_bits()
    );
    assert_eq!(
        scheduler::next_model_target(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        f64::NEG_INFINITY
    );
    assert_eq!(
        scheduler::next_model_target(f64::INFINITY, 100.0, 100.08),
        f64::INFINITY
    );
}

#[test]
fn scheduler_stdio_keeps_its_explicit_finite_wire_domain() {
    for request in ["NaN 0 0\n", "0 NaN 0\n", "0 0 inf\n"] {
        let mut output = Vec::new();
        assert!(scheduler::serve(Cursor::new(request), &mut output).is_err());
        assert!(output.is_empty());
    }
}

fn check(name: &str, argv: Vec<String>) -> Check {
    Check {
        name: name.into(),
        command: CommandSpec::new(argv, Duration::from_secs(5)),
        output_option: "--output".into(),
        growth_bytes: 0,
        scheduler_calls: None,
    }
}

fn later(sentinel: &Path) -> Check {
    check(
        "later",
        vec![
            "/bin/sh".into(),
            "-c".into(),
            ": > \"$1\"".into(),
            "planner-sentinel".into(),
            sentinel.to_str().unwrap().into(),
        ],
    )
}

#[test]
fn failed_comparison_is_recorded_and_stops_before_the_next_check() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("result");
    let sentinel = temp.path().join("sentinel");
    let checks = [
        check(
            "first",
            vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf out; printf err >&2; exit 42".into(),
            ],
        ),
        later(&sentinel),
    ];
    let error = run_checks(&checks, &output, temp.path(), &mut NativeRunner).unwrap_err();
    assert_eq!(error.downcast_ref::<CheckFailure>().unwrap().returncode, 42);
    assert!(!sentinel.exists());
    let rows: Value =
        serde_json::from_slice(&fs::read(output.join("commands.json")).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["name"], "first");
    assert_eq!(rows[0]["returncode"], 42);
    assert!(rows[0]["free_before"].as_u64().unwrap() > 0);
    assert_eq!(fs::read(output.join("first.log")).unwrap(), b"outerr");
    assert!(!output.join("later.log").exists());
    assert!(!output.join("result.json").exists());
}

#[test]
fn registered_controller_returns_the_first_real_child_status_without_running_later_leaves() {
    let temp = tempfile::tempdir().unwrap();
    let python = temp.path().join("original-python-fixture");
    fs::write(&python, "#!/bin/sh\nexit 42\n").unwrap();
    fs::set_permissions(&python, fs::Permissions::from_mode(0o755)).unwrap();
    let output = temp.path().join("result");
    let mut argv = vec![env!("CARGO_BIN_EXE_check-plannerd-ci").into()];
    for (name, value) in [
        ("--root", root()),
        ("--python", python),
        ("--bin-dir", temp.path().join("bin")),
        ("--artifact", temp.path().join("artifact")),
        ("--source-native", temp.path().join("source")),
        ("--binding", temp.path().join("params.so")),
        ("--output", output.clone()),
    ] {
        argv.extend([name.into(), value.to_str().unwrap().into()]);
    }
    argv.extend(["--timeout-seconds".into(), "5".into()]);
    for name in CHECK_NAMES {
        argv.extend(["--growth-mib".into(), format!("{name}=0")]);
    }
    let result = NativeRunner
        .run(&CommandSpec::new(argv, Duration::from_secs(10)))
        .unwrap();
    assert_eq!(result.code, 42, "{result:?}");
    let rows: Value =
        serde_json::from_slice(&fs::read(output.join("commands.json")).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["name"], "abi");
    assert_eq!(rows[0]["returncode"], 42);
    assert!(!output.join("policy.log").exists());
    assert!(!output.join("result.json").exists());
}

#[test]
fn operation_growth_denial_precedes_any_child_execution() {
    let temp = tempfile::tempdir().unwrap();
    let sentinel = temp.path().join("sentinel");
    let mut command = later(&sentinel);
    command.growth_bytes = u64::MAX;
    let output = temp.path().join("result");
    assert!(run_checks(&[command], &output, temp.path(), &mut NativeRunner).is_err());
    assert!(!sentinel.exists());
    assert!(!output.join("commands.json").exists());
}

#[test]
fn ipc_success_without_native_scheduler_use_is_rejected_before_the_next_check() {
    let temp = tempfile::tempdir().unwrap();
    let sentinel = temp.path().join("sentinel");
    let mut ipc = check("ipc", vec!["/bin/sh".into(), "-c".into(), "exit 0".into()]);
    ipc.scheduler_calls = Some(160);
    let output = temp.path().join("result");
    assert!(run_checks(
        &[ipc, later(&sentinel)],
        &output,
        temp.path(),
        &mut NativeRunner
    )
    .is_err());
    assert!(!sentinel.exists());
    assert!(!output.join("result.json").exists());
}

#[test]
fn registered_native_scheduler_streams_real_requests_and_retains_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = temp.path().join("scheduler.tsv");
    let mut command = CommandSpec::new(
        vec![
            env!("CARGO_BIN_EXE_check-plannerd-ci").into(),
            "schedule".into(),
        ],
        Duration::from_secs(5),
    );
    command.input = Some(b"100.05\t100.0\t100.08\n100.05\t100.04\t100.045\n".to_vec());
    command.env.insert(
        "PLANNER_NATIVE_SCHEDULER_EVIDENCE".into(),
        evidence.to_str().unwrap().into(),
    );
    let result = NativeRunner.run(&command).unwrap();
    assert_eq!(result.code, 0, "{result:?}");
    let actual = std::str::from_utf8(&result.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<f64>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), 2);
    for (actual, expected) in actual.into_iter().zip([100.13, 100.09]) {
        assert!((actual - expected).abs() <= 1e-12);
    }
    scheduler::verify_evidence(&evidence, 2).unwrap();
    assert!(scheduler::verify_evidence(&evidence, 160).is_err());
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn inputs(temp: &Path, python: PathBuf) -> Inputs {
    Inputs {
        root: root(),
        python,
        binaries: temp.join("bin"),
        artifact: temp.join("artifact"),
        source: temp.join("source"),
        binding: temp.join("params.so"),
        scheduler: env!("CARGO_BIN_EXE_check-plannerd-ci").into(),
    }
}
fn growth() -> BTreeMap<String, u64> {
    CHECK_NAMES
        .map(|name| (name.into(), 0))
        .into_iter()
        .collect()
}

#[test]
fn twelve_actual_leaf_commands_require_complete_operation_growth_and_scheduler_binding() {
    let temp = tempfile::tempdir().unwrap();
    let inputs = inputs(temp.path(), "/reference/python".into());
    let checks = plan::runtime_checks(&inputs, &growth(), Duration::from_secs(5)).unwrap();
    assert_eq!(
        checks
            .iter()
            .map(|check| check.name.as_str())
            .collect::<Vec<_>>(),
        CHECK_NAMES
    );
    assert_eq!(checks[10].scheduler_calls, Some(160));
    assert_eq!(
        checks[10].command.env["PLANNER_NATIVE_SCHEDULER"],
        inputs.scheduler.to_str().unwrap()
    );
    let mut incomplete = growth();
    incomplete.remove("owner");
    assert!(plan::runtime_checks(&inputs, &incomplete, Duration::from_secs(5)).is_err());
    assert!(plan::runtime_checks(&inputs, &growth(), Duration::ZERO).is_err());
}

#[test]
fn memory_plan_requires_only_abi_owner_fault_and_preserves_sanitized_wrapper_paths() {
    let temp = tempfile::tempdir().unwrap();
    let mut inputs = inputs(temp.path(), "/reference/python".into());
    inputs.binaries = temp.path().join("wrappers");
    let memory_growth = MEMORY_CHECK_NAMES
        .map(|name| (name.into(), 128))
        .into_iter()
        .collect();
    let checks = plan::memory_checks(&inputs, &memory_growth, Duration::from_secs(5)).unwrap();
    assert_eq!(
        checks
            .iter()
            .map(|check| check.name.as_str())
            .collect::<Vec<_>>(),
        MEMORY_CHECK_NAMES
    );
    for (check, trace) in checks.iter().zip(["solver", "owner", "runtime"]) {
        assert!(check.command.argv.contains(
            &inputs
                .binaries
                .join("examples")
                .join(format!("plannerd_{trace}_trace"))
                .to_str()
                .unwrap()
                .into()
        ));
        assert_eq!(check.scheduler_calls, None);
        assert_eq!(check.growth_bytes, 128);
    }
    assert!(plan::memory_checks(&inputs, &growth(), Duration::from_secs(5)).is_err());
}

fn memory_cli_fixture(fail_owner: bool) -> (tempfile::TempDir, Value, i32) {
    let temp = tempfile::tempdir().unwrap();
    let python = temp.path().join("original-python-fixture");
    let body = if fail_owner {
        "#!/bin/sh\ncase \"$3\" in */plannerd_owner_source.py) exit 42;; esac\nexit 0\n"
    } else {
        "#!/bin/sh\nexit 0\n"
    };
    fs::write(&python, body).unwrap();
    fs::set_permissions(&python, fs::Permissions::from_mode(0o755)).unwrap();
    let output = temp.path().join("comparison");
    let mut argv = vec![
        env!("CARGO_BIN_EXE_check-plannerd-ci").into(),
        "memory".into(),
    ];
    for (name, value) in [
        ("--root", root()),
        ("--python", python),
        ("--bin-dir", temp.path().join("wrappers")),
        ("--artifact", temp.path().join("sanitized")),
        ("--source-native", temp.path().join("source")),
        ("--binding", temp.path().join("params.so")),
        ("--output", output.clone()),
    ] {
        argv.extend([name.into(), value.to_str().unwrap().into()]);
    }
    argv.extend(["--timeout-seconds".into(), "5".into()]);
    for name in MEMORY_CHECK_NAMES {
        argv.extend(["--growth-mib".into(), format!("{name}=0")]);
    }
    let result = NativeRunner
        .run(&CommandSpec::new(argv, Duration::from_secs(10)))
        .unwrap();
    let rows = serde_json::from_slice(&fs::read(output.join("commands.json")).unwrap()).unwrap();
    assert!(
        !output.join("result.json").exists(),
        "memory wrapper owns the sanitizer receipt"
    );
    (temp, rows, result.code)
}

#[test]
fn registered_memory_controller_executes_three_checks_without_a_full_runtime_receipt() {
    let (_temp, rows, code) = memory_cli_fixture(false);
    assert_eq!(code, 0);
    assert_eq!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        MEMORY_CHECK_NAMES
    );
    assert!(rows
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["returncode"] == 0));
}

#[test]
fn registered_memory_controller_propagates_the_first_failed_leaf_and_skips_fault() {
    let (temp, rows, code) = memory_cli_fixture(true);
    assert_eq!(code, 42);
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["name"], "abi");
    assert_eq!(rows[1]["name"], "owner");
    assert_eq!(rows[1]["returncode"], 42);
    assert!(!temp.path().join("comparison/fault.log").exists());
}
