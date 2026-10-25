use openpilot_dev_tools::{
    planner_ci_controller::{
        plan::{self, Inputs, CHECK_NAMES},
        run_checks, scheduler, Check, CheckFailure,
    },
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use serde_json::{json, Value};
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

// 아래 원본 실행은 Actions에서 --ignored로 따로 수행한다. pure unit에는 Python이 없다.
fn reference(code: &str, arguments: &[String], input: Option<Vec<u8>>) -> Value {
    let python = std::env::var("PLANNER_REFERENCE_PYTHON")
        .expect("Actions must supply the admitted original Python executable");
    let mut argv = vec![
        python,
        "-I".into(),
        "-c".into(),
        code.into(),
        root().to_str().unwrap().into(),
    ];
    argv.extend_from_slice(arguments);
    let mut command = CommandSpec::new(argv, Duration::from_secs(30));
    command.input = input;
    let result = NativeRunner.run(&command).unwrap();
    assert_eq!(result.code, 0, "{result:?}");
    serde_json::from_str(
        std::str::from_utf8(&result.stdout)
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap()
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
