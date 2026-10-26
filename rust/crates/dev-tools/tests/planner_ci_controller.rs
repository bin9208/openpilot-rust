use openpilot_dev_tools::{
    planner_ci_controller::{
        plan::{self, Inputs, NativeIpcInputs, CHECK_NAMES, MEMORY_CHECK_NAMES},
        run_checks, scheduler, Check, CheckFailure,
    },
    qa_process::{CaptureError, CommandSpec, NativeRunner, Runner},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
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

fn ipc_group_fixture(directory: &Path) -> (Check, PathBuf) {
    let output = directory.join("comparison");
    let inputs = inputs(directory, "/unused/original-python".into());
    let check = plan::runtime_checks(&inputs, &growth(), Duration::from_secs(2))
        .unwrap()
        .remove(10);
    let full_argv = check
        .invocation(&output, &inputs.root)
        .unwrap()
        .effective_argv();
    assert_eq!(full_argv.len(), 36); // ELF, ipc, 기존 17개 option/value 쌍.
    assert_eq!(full_argv[0], inputs.scheduler.as_os_str());
    assert_eq!(full_argv[1], "ipc");
    let mut invocation = check.command.clone();
    let mut argv = invocation.effective_argv();
    // actual IPC argv를 보존한 process fixture다. 모델·원본·compiler를 실행하지 않는다.
    // NativeRunner가 만든 leader PGID를 네 종류 자손이 그대로 상속하는지 기록한다.
    let script = r#"
set -euo pipefail
[[ $# == 34 ]]
output=
while (($#)); do
  [[ $1 == --* ]]
  if [[ $1 == --output ]]; then output=$2; fi
  shift 2
done
[[ -n $output ]]
/usr/bin/mkdir "$output"
record() {
  local role=$1 pid=$2 row rest
  local -a fields
  IFS= read -r row < "/proc/$pid/stat"
  rest=${row##*) }; read -r -a fields <<< "$rest"
  [[ ${fields[2]} == "$BASHPID" && ${fields[19]} =~ ^[0-9]+$ ]]
  printf '%s %s %s %s\n' "$role" "$pid" "${fields[2]}" "${fields[19]}" >> "$output/ready.tmp"
}
record leader "$BASHPID"
for role in cpp-peer daemon scheduler compiler; do
  /usr/bin/sleep 30 &
  record "$role" "$!"
done
/usr/bin/mv "$output/ready.tmp" "$output/ready"
wait
"#;
    argv.splice(
        0..2,
        [
            "/usr/bin/bash".into(),
            "-c".into(),
            script.into(),
            "ipc-process-fixture".into(),
        ],
    );
    invocation.os_argv = Some(argv);
    (
        Check {
            command: invocation,
            ..check
        },
        output.join("ipc/ready"),
    )
}

fn assert_ipc_group_terminated(ready: &Path) {
    let rows = fs::read_to_string(ready).unwrap();
    let rows = rows
        .lines()
        .map(|row| row.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 5);
    let leader = rows[0][1];
    for (row, role) in rows
        .iter()
        .zip(["leader", "cpp-peer", "daemon", "scheduler", "compiler"])
    {
        assert_eq!(row.len(), 4);
        assert_eq!(row[0], role);
        assert_eq!(row[2], leader);
        let path = format!("/proc/{}/stat", row[1]);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match fs::read_to_string(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => panic!("cannot observe owned IPC fixture PID: {error}"),
                Ok(stat) => {
                    let fields = stat
                        .rsplit_once(") ")
                        .unwrap()
                        .1
                        .split_whitespace()
                        .collect::<Vec<_>>();
                    if fields[19] != row[3] {
                        break;
                    } // 재사용된 PID를 소유 자손으로 세지 않는다.
                    if role != "leader" && matches!(fields[0], "Z" | "X") {
                        break;
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "owned IPC {role} survived or leader was not reaped"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    // 직접 leader wait와 같은 PGID 자손의 비생존만 검사한다. 전역 adoption/reap 증거가 아니다.
}

fn ipc_timeout_and_controlled_cancel_keep_one_owned_group() {
    let temp = tempfile::tempdir().unwrap();
    let (ipc, ready) = ipc_group_fixture(temp.path());
    let sentinel = temp.path().join("later");
    let output = temp.path().join("comparison");
    let error = run_checks(
        &[ipc, later(&sentinel)],
        &output,
        &root(),
        &mut NativeRunner,
    )
    .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<CaptureError>(),
        Some(CaptureError::Timeout { .. })
    ));
    assert!(!sentinel.exists());
    assert!(!output.join("result.json").exists());
    assert!(output.join("ipc.log").exists());
    assert_ipc_group_terminated(&ready);

    let temp = tempfile::tempdir().unwrap();
    let (ipc, ready) = ipc_group_fixture(temp.path());
    let cancelled = AtomicBool::new(false);
    let output = temp.path().join("comparison");
    fs::create_dir(&output).unwrap();
    let mut command = ipc.invocation(&output, &root()).unwrap();
    command.timeout = Duration::from_secs(5);
    std::thread::scope(|scope| {
        let cancel = scope.spawn(|| {
            let deadline = Instant::now() + Duration::from_secs(3);
            while fs::read_to_string(&ready).map_or(true, |rows| rows.lines().count() != 5) {
                assert!(
                    Instant::now() < deadline,
                    "IPC cancellation fixture not ready"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            cancelled.store(true, Ordering::Release);
        });
        let error = NativeRunner
            .run_controlled(
                &command,
                Instant::now() + Duration::from_secs(5),
                &cancelled,
            )
            .unwrap_err();
        cancel.join().unwrap();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::Interrupted
        );
    });
    assert_ipc_group_terminated(&ready);
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
    ipc_timeout_and_controlled_cancel_keep_one_owned_group();
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
    ipc_point_of_use_preserves_prefix_and_upfront_growth();
}

fn ipc_point_of_use_preserves_prefix_and_upfront_growth() {
    let temp = tempfile::tempdir().unwrap();
    let python = temp.path().join("prefix-leaf");
    let calls = temp.path().join("calls");
    fs::write(
        &python,
        format!("#!/bin/sh\nprintf leaf >> '{}'\nexit 0\n", calls.display()),
    )
    .unwrap();
    fs::set_permissions(&python, fs::Permissions::from_mode(0o755)).unwrap();
    let output = temp.path().join("comparison");
    let mut argv = vec![env!("CARGO_BIN_EXE_check-plannerd-ci").into()];
    for (key, value) in [
        ("--root", root()),
        ("--python", python),
        ("--bin-dir", temp.path().join("bin")),
        ("--artifact", temp.path().join("artifact")),
        ("--source-native", temp.path().join("source")),
        ("--binding", temp.path().join("params.so")),
        ("--output", output.clone()),
    ] {
        argv.extend([key.into(), value.to_str().unwrap().into()]);
    }
    argv.extend(["--timeout-seconds".into(), "5".into()]);
    for name in CHECK_NAMES {
        argv.extend(["--growth-mib".into(), format!("{name}=0")]);
    }
    let mut command = CommandSpec::new(argv, Duration::from_secs(10));
    command.env_remove.push("PLANNER_NATIVE_IPC_PROFILE".into());
    let result = NativeRunner.run(&command).unwrap();
    assert_eq!(result.code, 1, "{result:?}");
    let error = if cfg!(feature = "planner-ipc-native") {
        "prepared runtime IPC requires PLANNER_NATIVE_IPC_PROFILE"
    } else {
        "runtime IPC requires the planner-ipc-native feature"
    };
    assert!(
        String::from_utf8_lossy(&result.stderr).contains(error),
        "{result:?}"
    );
    let rows: Value =
        serde_json::from_slice(&fs::read(output.join("commands.json")).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 10);
    for (row, name) in rows.as_array().unwrap().iter().zip(&CHECK_NAMES[..10]) {
        assert_eq!(row["name"].as_str(), Some(*name));
        assert_eq!(row["returncode"], 0);
    }
    assert_eq!(fs::read(&calls).unwrap(), b"leaf".repeat(10));
    for name in ["ipc.log", "lifecycle.log", "result.json"] {
        assert!(!output.join(name).exists());
    }

    // IPC context는 늦추지만 전체 growth12 exact 검증은 첫 prefix 실행 전이다.
    let invalid_output = temp.path().join("missing-growth");
    let output_index = command
        .argv
        .iter()
        .position(|arg| arg == "--output")
        .unwrap()
        + 1;
    command.argv[output_index] = invalid_output.to_str().unwrap().into();
    let ipc_growth = command.argv.iter().position(|arg| arg == "ipc=0").unwrap();
    command.argv.drain(ipc_growth - 1..=ipc_growth);
    let result = NativeRunner.run(&command).unwrap();
    assert_eq!(result.code, 1);
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("growth admission must name every selected Planner operation exactly once"));
    assert!(!invalid_output.exists());
    assert_eq!(fs::read(&calls).unwrap(), b"leaf".repeat(10));
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
        native_ipc: Some(NativeIpcInputs {
            original_ipc_binding: temp.join("ipc_pyx.fixture.so"),
            compiler: temp.join("g++"),
            corpus: temp.join("corpus"),
            corpus_audit: temp.join("corpus/audit.json"),
            corpus_audit_sha256: "a".repeat(64),
            timeout: Duration::from_secs(2),
            growth_mib: 512,
            peer_build_growth_mib: 128,
        }),
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
    let output = temp.path().join("planner-comparison");
    let ipc = checks[10].invocation(&output, &inputs.root).unwrap();
    let context = inputs.native_ipc.as_ref().unwrap();
    assert_eq!(
        ipc.effective_argv(),
        [
            inputs.scheduler.clone(),
            PathBuf::from("ipc"),
            "--root".into(),
            inputs.root.clone(),
            "--python".into(),
            inputs.python.clone(),
            "--binary".into(),
            inputs.binaries.join("openpilot-plannerd"),
            "--artifact".into(),
            inputs.artifact.clone(),
            "--binding".into(),
            inputs.binding.clone(),
            "--source-native".into(),
            inputs.source.clone(),
            "--original-ipc-binding".into(),
            context.original_ipc_binding.clone(),
            "--scheduler".into(),
            inputs.scheduler.clone(),
            "--compiler".into(),
            context.compiler.clone(),
            "--corpus".into(),
            context.corpus.clone(),
            "--corpus-audit".into(),
            context.corpus_audit.clone(),
            "--corpus-audit-sha256".into(),
            "a".repeat(64).into(),
            "--timeout-seconds".into(),
            "2".into(),
            "--growth-mib".into(),
            "512".into(),
            "--peer-build-growth-mib".into(),
            "128".into(),
            PathBuf::from("--output"),
            output.join("ipc"),
            "--evidence".into(),
            output.join("ipc-scheduler.tsv"),
        ]
        .map(PathBuf::into_os_string)
    );
    assert_eq!(
        ipc.env,
        BTreeMap::from([(
            "PLANNER_NATIVE_SCHEDULER".into(),
            inputs.scheduler.to_str().unwrap().into(),
        )])
    );
    assert_eq!(ipc.cwd.as_deref(), Some(inputs.root.as_path()));
    assert!(ipc.merge_output);
    for (index, check) in checks.iter().enumerate() {
        assert_eq!(check.command.timeout, Duration::from_secs(5));
        assert_eq!(check.growth_bytes, 0);
        if index != 10 {
            assert_eq!(check.scheduler_calls, None);
            assert!(check.command.env.is_empty());
        }
    }
    let mut incomplete = growth();
    incomplete.remove("owner");
    assert!(plan::runtime_checks(&inputs, &incomplete, Duration::from_secs(5)).is_err());
    assert!(plan::runtime_checks(&inputs, &growth(), Duration::ZERO).is_err());
    for invalid in ["absent", "path", "sha", "timeout", "growth", "peer-growth"] {
        let mut bad = inputs.clone();
        if invalid == "absent" {
            bad.native_ipc = None;
        } else {
            let context = bad.native_ipc.as_mut().unwrap();
            match invalid {
                "path" => context.compiler = "relative-compiler".into(),
                "sha" => context.corpus_audit_sha256 = "A".repeat(64),
                "timeout" => context.timeout = Duration::from_secs(6),
                "growth" => context.growth_mib = 0,
                _ => context.peer_build_growth_mib = 0,
            }
        }
        assert!(
            plan::runtime_checks(&bad, &growth(), Duration::from_secs(5)).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn memory_plan_requires_only_abi_owner_fault_and_preserves_sanitized_wrapper_paths() {
    let temp = tempfile::tempdir().unwrap();
    let mut inputs = inputs(temp.path(), "/reference/python".into());
    inputs.binaries = temp.path().join("wrappers");
    inputs.native_ipc = None;
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
