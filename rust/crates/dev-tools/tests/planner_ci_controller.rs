use openpilot_dev_tools::{
    planner_ci_controller::{
        plan::{self, Inputs, CHECK_NAMES, MEMORY_CHECK_NAMES},
        run_checks, scheduler, Check, CheckFailure,
    },
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
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

// 아래 원본 실행은 Actions에서 --ignored로 따로 수행한다. pure unit에는 Python이 없다.
fn original_file_sha256(path: &Path) -> String {
    let mut file = fs::File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    format!("{:x}", hash.finalize())
}

fn record_original_observation(name: &str, observation: Value) {
    const NAMES: [&str; 5] = [
        "source_scheduler_matches_native_protocol_without_reimplementing_the_oracle",
        "source_public_scheduler_preserves_python_ordered_comparison_at_float_boundaries",
        "source_runtime_plan_matches_every_native_leaf_and_argument",
        "source_memory_subset_matches_every_native_sanitizer_leaf_argument",
        "source_failure_forward_matches_native_controller_and_raw_log",
    ];
    const ONE_MIB: u64 = 1024 * 1024;
    assert!(NAMES.contains(&name));
    let job =
        PathBuf::from(std::env::var_os("CI_JOB_TEMP").expect("Actions owned job path is required"));
    assert!(job.is_absolute());
    let directory = job.join("proof/planner-original-corpus");
    fs::create_dir_all(&directory).unwrap();
    assert!(fs::symlink_metadata(&directory)
        .unwrap()
        .file_type()
        .is_dir());
    assert_eq!(fs::canonicalize(&directory).unwrap(), directory);
    let mut bytes = serde_json::to_vec_pretty(&observation).unwrap();
    bytes.push(b'\n');
    assert!(
        bytes.len() as u64 <= ONE_MIB,
        "one original observation exceeds 1 MiB"
    );
    let mut total = 0;
    for entry in fs::read_dir(&directory).unwrap() {
        let entry = entry.unwrap();
        let metadata = fs::symlink_metadata(entry.path()).unwrap();
        assert!(metadata.file_type().is_file());
        assert!(NAMES
            .iter()
            .any(|name| entry.file_name() == format!("{name}.json").as_str()));
        assert!(metadata.len() <= ONE_MIB);
        total += metadata.len();
    }
    // 5개 이름 x 1MiB 제한은 병렬 capture에도 8MiB 상한을 넘지 않는다.
    assert!(total + bytes.len() as u64 <= 8 * ONE_MIB);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join(format!("{name}.json")))
        .unwrap();
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
}

fn reference(
    name: &str,
    cases: Value,
    code: &str,
    arguments: &[String],
    input: Option<Vec<u8>>,
) -> Value {
    let python = std::env::var("PLANNER_REFERENCE_PYTHON")
        .expect("Actions must supply the admitted original Python executable");
    let source_root = root();
    let source_paths = [
        "rust/tools/check_plannerd_ci.py",
        "rust/tools/plannerd_ipc.py",
        "rust/crates/dev-tools/tests/planner_ci_controller.rs",
        "rust/crates/dev-tools/src/planner_ci_controller.rs",
        "rust/crates/dev-tools/src/planner_ci_controller/cli.rs",
        "rust/crates/dev-tools/src/planner_ci_controller/plan.rs",
        "rust/crates/dev-tools/src/planner_ci_controller/scheduler.rs",
        "rust/crates/dev-tools/src/bin/check_plannerd_ci.rs",
        "rust/crates/dev-tools/src/qa_process.rs",
        "rust/crates/disk-space/src/lib.rs",
    ];
    let source_hashes = || {
        source_paths
            .iter()
            .map(|path| {
                (
                    (*path).to_owned(),
                    original_file_sha256(&source_root.join(path)),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let before = source_hashes();
    let python_sha256 = original_file_sha256(Path::new(&python));
    let proof =
        PathBuf::from(std::env::var_os("CI_JOB_TEMP").expect("Actions proof path is required"))
            .join("proof");
    let read_proof = |file: &str| {
        let path = proof.join(file);
        assert!(fs::metadata(&path).unwrap().len() <= 4096);
        fs::read_to_string(path).unwrap()
    };
    // 같은 cohort의 기존 관측을 재사용한다. Python --version/process를 새로 실행하지 않는다.
    let python_hash_proof = read_proof("planner-oracle-python.sha256");
    let python_version_proof = read_proof("planner-oracle-python.version");
    assert_eq!(
        python_hash_proof.trim_end(),
        format!("{python_sha256}  {python}")
    );
    assert!(python_version_proof.trim().starts_with("Python "));
    let github_sha = std::env::var("GITHUB_SHA").expect("GITHUB_SHA is required");
    assert!(github_sha.len() == 40 && github_sha.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let mut argv = vec![
        python.clone(),
        "-I".into(),
        "-c".into(),
        code.into(),
        root().to_str().unwrap().into(),
    ];
    argv.extend_from_slice(arguments);
    let invocation = argv.clone();
    let stdin = input
        .as_ref()
        .map(|bytes| std::str::from_utf8(bytes).unwrap().to_owned());
    let mut command = CommandSpec::new(argv, Duration::from_secs(30));
    command.input = input;
    let result = NativeRunner.run(&command).unwrap();
    assert_eq!(result.code, 0, "{result:?}");
    let original: Value = serde_json::from_str(
        std::str::from_utf8(&result.stdout)
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    if let Some(observed_python) = original.get("python") {
        assert_eq!(observed_python.as_str(), Some(python.as_str()));
    }
    assert_eq!(
        source_hashes(),
        before,
        "oracle/Rust source changed during capture"
    );
    assert_eq!(original_file_sha256(Path::new(&python)), python_sha256);
    assert_eq!(
        read_proof("planner-oracle-python.sha256"),
        python_hash_proof
    );
    assert_eq!(
        read_proof("planner-oracle-python.version"),
        python_version_proof
    );
    let original_log = if name == "source_failure_forward_matches_native_controller_and_raw_log" {
        Some(fs::read_to_string(Path::new(&arguments[2]).join("first.log")).unwrap())
    } else {
        None
    };
    record_original_observation(
        name,
        json!({
            "schema_version":1, "kind":"independent-python-observation", "test_name":name,
            "github_sha":github_sha, "source_sha256":before,
            "python":{"executable":python,"executable_sha256":python_sha256,
                "sha256_proof":python_hash_proof,"version_proof":python_version_proof},
            "cases":cases,"argv":invocation,"stdin_utf8":stdin,
            "original_result":original,"original_stdout_utf8":std::str::from_utf8(&result.stdout).unwrap(),
            "original_stderr_utf8":std::str::from_utf8(&result.stderr).unwrap(),
            "original_returncode":result.code,"original_first_log_utf8":original_log,
            "native_comparison":"enclosing test assertions must pass; observation alone is not acceptance"
        }),
    );
    original
}

#[test]
#[ignore = "Actions 원본 pacing 비교; PLANNER_REFERENCE_PYTHON 필요"]
fn source_scheduler_matches_native_protocol_without_reimplementing_the_oracle() {
    let mut cases = Vec::new();
    for sent in [0.001, 100.0, 1_000_000.0] {
        for wake in [0.0, 0.04, 0.1] {
            for work in [0.003, 0.013, 0.025, 0.049, 0.05, 0.0500000001, 0.08, 1.0] {
                cases.push([sent + 0.05 - wake, sent, sent + work]);
            }
        }
    }
    cases.extend(signed_zero_cases());
    let source = reference(
        "source_scheduler_matches_native_protocol_without_reimplementing_the_oracle",
        json!({"encoding":"ieee754-binary64-bits", "timestamps":cases.iter().map(|case| case.map(f64::to_bits)).collect::<Vec<_>>()}),
        r#"
import ast, json, pathlib, struct, sys
tree = ast.parse((pathlib.Path(sys.argv[1]) / 'rust/tools/plannerd_ipc.py').read_text())
function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'next_model_target')
namespace = {}
exec(compile(ast.Module([function], type_ignores=[]), '<original IPC scheduler>', 'exec'), namespace)
print(json.dumps([struct.unpack('>Q', struct.pack('>d', namespace['next_model_target'](*case)))[0] for case in json.load(sys.stdin)]))
"#,
        &[],
        Some(serde_json::to_vec(&cases).unwrap()),
    );
    let actual = endpoint(&cases);
    assert_eq!(source.as_array().unwrap().len(), actual.len());
    for (expected, actual) in source.as_array().unwrap().iter().zip(actual) {
        assert_eq!(expected.as_u64().unwrap(), actual.to_bits());
    }
}

#[test]
#[ignore = "Actions 원본 public helper NaN/signed-zero 비교; PLANNER_REFERENCE_PYTHON 필요"]
fn source_public_scheduler_preserves_python_ordered_comparison_at_float_boundaries() {
    let nan = f64::from_bits(0x7ff8_0000_0000_0042);
    let mut cases = signed_zero_cases().to_vec();
    cases.extend([
        [nan, 0.0, 0.08],
        [100.05, nan, 100.08],
        [100.05, 100.0, nan],
        [f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY],
        [f64::INFINITY, 100.0, 100.08],
    ]);
    let bits = cases
        .iter()
        .map(|case| case.map(f64::to_bits))
        .collect::<Vec<_>>();
    let source = reference(
        "source_public_scheduler_preserves_python_ordered_comparison_at_float_boundaries",
        json!({"encoding":"ieee754-binary64-bits", "timestamps":bits}),
        r#"
import ast, json, pathlib, struct, sys
tree = ast.parse((pathlib.Path(sys.argv[1]) / 'rust/tools/plannerd_ipc.py').read_text())
function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'next_model_target')
namespace = {}
exec(compile(ast.Module([function], type_ignores=[]), '<original public IPC scheduler>', 'exec'), namespace)
cases = [[struct.unpack('>d', struct.pack('>Q', bits))[0] for bits in case] for case in json.load(sys.stdin)]
print(json.dumps([struct.unpack('>Q', struct.pack('>d', namespace['next_model_target'](*case)))[0] for case in cases]))
"#,
        &[],
        Some(serde_json::to_vec(&bits).unwrap()),
    );
    for (expected, case) in source.as_array().unwrap().iter().zip(&cases) {
        assert_eq!(
            expected.as_u64().unwrap(),
            scheduler::next_model_target(case[0], case[1], case[2]).to_bits()
        );
    }
    assert_eq!(source.as_array().unwrap().len(), cases.len());
}

#[test]
#[ignore = "Actions 원본 12-leaf argv 비교; PLANNER_REFERENCE_PYTHON 필요"]
fn source_runtime_plan_matches_every_native_leaf_and_argument() {
    let temp = tempfile::tempdir().unwrap();
    let mut inputs = inputs(temp.path(), "/unused".into());
    let output = temp.path().join("output");
    let config = json!({"binaries":inputs.binaries, "artifact":inputs.artifact, "source":inputs.source, "binding":inputs.binding, "output":output});
    let source = reference(
        "source_runtime_plan_matches_every_native_leaf_and_argument",
        config.clone(),
        r#"
import json, pathlib, runpy, sys
ns = runpy.run_path(str(pathlib.Path(sys.argv[1]) / 'rust/tools/check_plannerd_ci.py'))
config = json.loads(sys.argv[2])
inputs = ns['Inputs'](*(pathlib.Path(config[key]) for key in ('binaries', 'artifact', 'source', 'binding')))
checks = [{'name': check.name, 'argv': [sys.executable, '-u', '-P', str(check.script), *check.arguments, check.output_option, str(pathlib.Path(config['output']) / check.name)]} for check in ns['runtime_checks'](inputs)]
print(json.dumps({'python': sys.executable, 'checks': checks}))
"#,
        &[config.to_string()],
        None,
    );
    inputs.python = source["python"].as_str().unwrap().into();
    let checks = plan::runtime_checks(&inputs, &growth(), Duration::from_secs(5)).unwrap();
    let actual = checks.iter().map(|check| {
        let command = check.invocation(&output, &inputs.root).unwrap();
        json!({"name":check.name, "argv":command.effective_argv().into_iter().map(|arg| arg.into_string().unwrap()).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    assert_eq!(json!(actual), source["checks"]);
}

#[test]
#[ignore = "Actions 원본 memory subset argv 비교; PLANNER_REFERENCE_PYTHON 필요"]
fn source_memory_subset_matches_every_native_sanitizer_leaf_argument() {
    let temp = tempfile::tempdir().unwrap();
    let mut inputs = inputs(temp.path(), "/unused".into());
    inputs.binaries = temp.path().join("wrappers");
    let output = temp.path().join("comparison");
    let config = json!({"binaries":inputs.binaries, "artifact":inputs.artifact, "source":inputs.source, "binding":inputs.binding, "output":output});
    let source = reference(
        "source_memory_subset_matches_every_native_sanitizer_leaf_argument",
        config.clone(),
        r#"
import json, pathlib, runpy, sys
ns = runpy.run_path(str(pathlib.Path(sys.argv[1]) / 'rust/tools/check_plannerd_ci.py'))
config = json.loads(sys.argv[2])
inputs = ns['Inputs'](*(pathlib.Path(config[key]) for key in ('binaries', 'artifact', 'source', 'binding')))
checks = [{'name': check.name, 'argv': [sys.executable, '-u', '-P', str(check.script), *check.arguments, check.output_option, str(pathlib.Path(config['output']) / check.name)]} for check in ns['runtime_checks'](inputs) if check.name in ('abi', 'owner', 'fault')]
print(json.dumps({'python': sys.executable, 'checks': checks}))
"#,
        &[config.to_string()],
        None,
    );
    inputs.python = source["python"].as_str().unwrap().into();
    let growth = MEMORY_CHECK_NAMES
        .map(|name| (name.into(), 0))
        .into_iter()
        .collect();
    let checks = plan::memory_checks(&inputs, &growth, Duration::from_secs(5)).unwrap();
    let actual = checks.iter().map(|check| {
        let command = check.invocation(&output, &inputs.root).unwrap();
        json!({"name":check.name, "argv":command.effective_argv().into_iter().map(|arg| arg.into_string().unwrap()).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    assert_eq!(json!(actual), source["checks"]);
}

#[test]
#[ignore = "Actions 원본 실패 기록/전파 비교; PLANNER_REFERENCE_PYTHON 필요"]
fn source_failure_forward_matches_native_controller_and_raw_log() {
    let temp = tempfile::tempdir().unwrap();
    let failing = temp.path().join("failing.py");
    let later = temp.path().join("later.py");
    let sentinel = temp.path().join("sentinel");
    fs::write(
        &failing,
        "import sys\nprint('out')\nprint('err', file=sys.stderr)\nraise SystemExit(42)\n",
    )
    .unwrap();
    fs::write(
        &later,
        "from pathlib import Path\nPath(__file__).with_name('sentinel').touch()\n",
    )
    .unwrap();
    let baseline = temp.path().join("source-result");
    let source = reference(
        "source_failure_forward_matches_native_controller_and_raw_log",
        json!({"failing_script":fs::read_to_string(&failing).unwrap(), "later_script":fs::read_to_string(&later).unwrap(),
            "failing_path":failing,"later_path":later,"sentinel_path":sentinel,"source_output":baseline,
            "disk_usage_free_override":1_u64 << 60}),
        r#"
import json, pathlib, runpy, subprocess, sys, types, unittest.mock
ns = runpy.run_path(str(pathlib.Path(sys.argv[1]) / 'rust/tools/check_plannerd_ci.py'))
checks = tuple(ns['Check'](name, pathlib.Path(path)) for name, path in zip(('first', 'later'), sys.argv[2:4]))
output = pathlib.Path(sys.argv[4])
# 기존 고정 admission을 이 실패 전파 비교에서만 독립시킨다. 함수/명령/실패는 원문이다.
with unittest.mock.patch.object(ns['shutil'], 'disk_usage', return_value=types.SimpleNamespace(free=1 << 60)):
    try:
        ns['run_checks'](checks, output)
    except subprocess.CalledProcessError as error:
        code = error.returncode
    else:
        raise AssertionError('original controller accepted the failing comparison')
print(json.dumps({'python': sys.executable, 'returncode': code, 'rows': json.loads((output / 'commands.json').read_text())}))
"#,
        &[
            failing.to_str().unwrap().into(),
            later.to_str().unwrap().into(),
            baseline.to_str().unwrap().into(),
        ],
        None,
    );
    assert!(!sentinel.exists());
    let python = source["python"].as_str().unwrap();
    let checks = [("first", &failing), ("later", &later)].map(|(name, script)| {
        check(
            name,
            vec![
                python.into(),
                "-u".into(),
                "-P".into(),
                script.to_str().unwrap().into(),
            ],
        )
    });
    let output = temp.path().join("native-result");
    let error = run_checks(&checks, &output, &root(), &mut NativeRunner).unwrap_err();
    assert_eq!(
        error.downcast_ref::<CheckFailure>().unwrap().returncode,
        source["returncode"].as_i64().unwrap() as i32
    );
    assert!(!sentinel.exists());
    let rows: Value =
        serde_json::from_slice(&fs::read(output.join("commands.json")).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["name"], source["rows"][0]["name"]);
    assert_eq!(rows[0]["returncode"], source["rows"][0]["returncode"]);
    let argv = rows[0]["argv"].as_array().unwrap();
    let baseline_argv = source["rows"][0]["argv"].as_array().unwrap();
    assert_eq!(
        &argv[..argv.len() - 1],
        &baseline_argv[..baseline_argv.len() - 1]
    );
    assert_eq!(
        fs::read(output.join("first.log")).unwrap(),
        fs::read(baseline.join("first.log")).unwrap()
    );
    assert!(!output.join("result.json").exists());
}
