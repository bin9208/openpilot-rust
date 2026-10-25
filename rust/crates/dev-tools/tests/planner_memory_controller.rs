use openpilot_dev_tools::{
    planner_ci_controller::{
        memory,
        plan::{Inputs, MEMORY_CHECK_NAMES},
        CheckFailure,
    },
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

const SCOPE: &str = "generated solver C is instrumented; allocator leak/ownership checks enabled; Rust and pinned external solver libraries are not instrumented";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn inputs(directory: &Path) -> Inputs {
    Inputs {
        root: directory.into(),
        python: directory.join("original-python-fixture"),
        binaries: directory.join("bin"),
        artifact: directory.join("artifact"),
        source: directory.join("source"),
        binding: directory.join("params.so"),
        scheduler: env!("CARGO_BIN_EXE_check-plannerd-ci").into(),
    }
}

fn fixture(directory: &Path) -> Inputs {
    let inputs = inputs(directory);
    fs::create_dir_all(inputs.binaries.join("examples")).unwrap();
    for name in ["solver", "owner", "runtime"] {
        fs::write(
            inputs
                .binaries
                .join(format!("examples/plannerd_{name}_trace")),
            name,
        )
        .unwrap();
    }
    fs::create_dir(&inputs.artifact).unwrap();
    fs::write(
        inputs.artifact.join("provenance.json"),
        br#"{"sanitizers":true,"generator_files":{"pinned":"unchanged"}}"#,
    )
    .unwrap();
    fs::write(
        inputs.artifact.join("manifest.json"),
        b"original manifest\n",
    )
    .unwrap();
    fs::write(directory.join("libasan.so"), b"original ASan library").unwrap();
    inputs
}

fn growth() -> BTreeMap<String, u64> {
    [("abi", 128), ("owner", 1024), ("fault", 1024)]
        .map(|(name, mib)| (name.into(), mib * 1024 * 1024))
        .into_iter()
        .collect()
}

#[derive(Default)]
struct Leaves {
    calls: Vec<CommandSpec>,
    fail_at: Option<usize>,
}
impl Runner for Leaves {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        let code = if self.fail_at == Some(self.calls.len()) {
            42
        } else {
            0
        };
        self.calls.push(command.clone());
        Ok(Captured {
            code,
            stdout: b"controlled leaf\n".to_vec(),
            stderr: Vec::new(),
        })
    }
}

#[test]
fn memory_orchestration_keeps_three_leaves_hashes_flags_and_limited_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let inputs = fixture(temp.path());
    let output = temp.path().join("output");
    let provenance = fs::read(inputs.artifact.join("provenance.json")).unwrap();
    let mut leaves = Leaves::default();
    let receipt = memory::run(
        &inputs,
        &temp.path().join("libasan.so"),
        &output,
        &growth(),
        Duration::from_secs(2700),
        &mut leaves,
    )
    .unwrap();
    assert_eq!(receipt["status"], "PASS");
    assert_eq!(receipt["checks"], json!(MEMORY_CHECK_NAMES));
    assert_eq!(receipt["scope"], SCOPE);
    assert!(receipt.get("source_commit").is_none());
    assert_eq!(
        receipt["asan_sha256"],
        digest(&temp.path().join("libasan.so")).unwrap()
    );
    assert_eq!(
        receipt["artifact_sha256"],
        digest(&inputs.artifact.join("manifest.json")).unwrap()
    );
    for (index, trace) in ["solver", "owner", "runtime"].iter().enumerate() {
        let binary = inputs
            .binaries
            .join(format!("examples/plannerd_{trace}_trace"))
            .canonicalize()
            .unwrap();
        assert_eq!(
            receipt["binary_sha256"][binary.to_str().unwrap()],
            digest(&binary).unwrap()
        );
        let wrapper = output.join(format!("wrappers/examples/plannerd_{trace}_trace"));
        let body = fs::read_to_string(&wrapper).unwrap();
        assert!(
            body.contains("ASAN_OPTIONS=detect_leaks=1:abort_on_error=1:strict_string_checks=1")
        );
        assert!(body.contains("UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1"));
        assert!(body.contains(&format!(
            "LD_PRELOAD={}",
            temp.path().join("libasan.so").display()
        )));
        assert!(body.ends_with(" \"$@\"\n"));
        assert_eq!(
            fs::metadata(wrapper).unwrap().permissions().mode() & 0o777,
            0o755
        );
        let call = &leaves.calls[index];
        assert_eq!(call.timeout, Duration::from_secs(2700));
        assert_eq!(call.cwd.as_ref(), Some(&inputs.root));
        assert!(
            call.env.is_empty(),
            "ASan belongs on the trace, not the Python leaf"
        );
        assert!(call.argv.contains(
            &output
                .join(format!("wrappers/examples/plannerd_{trace}_trace"))
                .to_str()
                .unwrap()
                .to_owned()
        ));
    }
    assert_eq!(leaves.calls.len(), 3);
    assert!(leaves.calls[2]
        .argv
        .ends_with(&["--faults".into(), "--frames".into(), "80".into()]));
    let records: Value =
        serde_json::from_slice(&fs::read(output.join("comparison/commands.json")).unwrap())
            .unwrap();
    for (record, name) in records.as_array().unwrap().iter().zip(MEMORY_CHECK_NAMES) {
        assert_eq!(record["name"], name);
        assert_eq!(record["growth_bytes"], growth()[name]);
        assert_eq!(record["returncode"], 0);
    }
    assert_eq!(
        fs::read(inputs.artifact.join("provenance.json")).unwrap(),
        provenance
    );
    assert!(!output.join("comparison/result.json").exists());
}

#[test]
fn memory_first_failure_42_stops_before_fault_and_never_writes_pass() {
    let temp = tempfile::tempdir().unwrap();
    let inputs = fixture(temp.path());
    let output = temp.path().join("output");
    let mut leaves = Leaves {
        fail_at: Some(1),
        ..Leaves::default()
    };
    let error = memory::run(
        &inputs,
        &temp.path().join("libasan.so"),
        &output,
        &growth(),
        Duration::from_secs(5),
        &mut leaves,
    )
    .unwrap_err();
    assert_eq!(error.downcast_ref::<CheckFailure>().unwrap().returncode, 42);
    assert_eq!(leaves.calls.len(), 2);
    let records: Value =
        serde_json::from_slice(&fs::read(output.join("comparison/commands.json")).unwrap())
            .unwrap();
    assert_eq!(records[1]["name"], "owner");
    assert_eq!(records[1]["returncode"], 42);
    assert!(!output.join("comparison/fault.log").exists());
    assert!(!output.join("result.json").exists());
}

#[test]
fn memory_receipt_requires_existing_complete_ordered_successful_records() {
    let temp = tempfile::tempdir().unwrap();
    let inputs = fixture(temp.path());
    let output = temp.path().join("output");
    fs::create_dir_all(output.join("comparison")).unwrap();
    let finish = || {
        memory::write_receipt(
            &inputs.artifact,
            &temp.path().join("libasan.so"),
            &output,
            &BTreeMap::new(),
        )
    };
    assert!(finish().is_err());
    for rows in [
        json!([]),
        json!([{"name":"abi","returncode":0},{"name":"owner","returncode":0}]),
        json!([{"name":"owner","returncode":0},{"name":"abi","returncode":0},{"name":"fault","returncode":0}]),
        json!([{"name":"abi","returncode":0},{"name":"owner","returncode":42},{"name":"fault","returncode":0}]),
    ] {
        fs::write(
            output.join("comparison/commands.json"),
            serde_json::to_vec(&rows).unwrap(),
        )
        .unwrap();
        assert!(finish().is_err());
        assert!(!output.join("result.json").exists());
    }
}

#[test]
fn memory_rejects_unsanitized_missing_library_and_reused_wrappers_before_any_leaf() {
    for mode in [
        "unsanitized",
        "missing-library",
        "reused-wrappers",
        "missing-binary",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let inputs = fixture(temp.path());
        let output = temp.path().join("output");
        match mode {
            "unsanitized" => fs::write(
                inputs.artifact.join("provenance.json"),
                br#"{"sanitizers":false}"#,
            )
            .unwrap(),
            "missing-library" => fs::remove_file(temp.path().join("libasan.so")).unwrap(),
            "missing-binary" => {
                fs::remove_file(inputs.binaries.join("examples/plannerd_owner_trace")).unwrap()
            }
            _ => fs::create_dir_all(output.join("wrappers/examples")).unwrap(),
        }
        let mut leaves = Leaves::default();
        assert!(
            memory::run(
                &inputs,
                &temp.path().join("libasan.so"),
                &output,
                &growth(),
                Duration::from_secs(5),
                &mut leaves
            )
            .is_err(),
            "{mode}"
        );
        assert!(leaves.calls.is_empty());
        assert!(!output.join("result.json").exists());
        if mode == "unsanitized" {
            assert!(!output.exists());
        }
    }
}

#[test]
fn memory_keeps_growth_and_timeout_admission_before_leaf_execution() {
    for mode in ["missing", "extra", "capacity", "timeout"] {
        let temp = tempfile::tempdir().unwrap();
        let inputs = fixture(temp.path());
        let mut budget = growth();
        let mut timeout = Duration::from_secs(5);
        match mode {
            "missing" => {
                budget.remove("owner");
            }
            "extra" => {
                budget.insert("main".into(), 0);
            }
            "capacity" => {
                budget.insert("abi".into(), u64::MAX);
            }
            _ => timeout = Duration::ZERO,
        }
        let mut leaves = Leaves::default();
        assert!(
            memory::run(
                &inputs,
                &temp.path().join("libasan.so"),
                &temp.path().join("output"),
                &budget,
                timeout,
                &mut leaves
            )
            .is_err(),
            "{mode}"
        );
        assert!(leaves.calls.is_empty());
        assert!(!temp.path().join("output/result.json").exists());
    }
}

fn cli(inputs: &Inputs, output: &Path, mode: &str, leaf_body: &str) -> Captured {
    fs::write(&inputs.python, format!("#!/bin/sh\n{leaf_body}\n")).unwrap();
    fs::set_permissions(&inputs.python, fs::Permissions::from_mode(0o755)).unwrap();
    let mut argv = vec![env!("CARGO_BIN_EXE_check-plannerd-ci").to_owned()];
    if mode == "memory" {
        argv.push(mode.into());
    }
    for (name, value) in [
        ("--root", inputs.root.clone()),
        ("--python", inputs.python.clone()),
        ("--bin-dir", inputs.binaries.clone()),
        ("--artifact", inputs.artifact.clone()),
        ("--source-native", inputs.source.clone()),
        ("--binding", inputs.binding.clone()),
        ("--output", output.into()),
        ("--asan-library", inputs.root.join("libasan.so")),
    ] {
        argv.extend([name.into(), value.to_str().unwrap().into()]);
    }
    argv.extend(["--timeout-seconds".into(), "5".into()]);
    for name in MEMORY_CHECK_NAMES {
        argv.extend(["--growth-mib".into(), format!("{name}=0")]);
    }
    NativeRunner
        .run(&CommandSpec::new(argv, Duration::from_secs(15)))
        .unwrap()
}

#[test]
fn registered_native_memory_cli_writes_only_the_limited_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let inputs = fixture(temp.path());
    let output = temp.path().join("output");
    let result = cli(&inputs, &output, "memory", "exit 0");
    assert_eq!(result.code, 0, "{result:?}");
    let receipt: Value =
        serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
    assert_eq!(receipt["checks"], json!(MEMORY_CHECK_NAMES));
    assert_eq!(receipt["scope"], SCOPE);
    assert!(!output.join("comparison/result.json").exists());
}

#[test]
fn registered_native_memory_cli_preserves_42_and_maps_signal_failure_to_one() {
    for (body, expected) in [
        (
            "case \"$3\" in */plannerd_owner_source.py) exit 42;; esac\nexit 0",
            42,
        ),
        ("kill -TERM $$", 1),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let inputs = fixture(temp.path());
        let output = temp.path().join("output");
        let result = cli(&inputs, &output, "memory", body);
        assert_eq!(result.code, expected, "{result:?}");
        assert!(!output.join("result.json").exists());
        assert!(!output.join("comparison/fault.log").exists());
    }
}

#[test]
fn runtime_rejects_asan_option_without_invoking_a_leaf() {
    let temp = tempfile::tempdir().unwrap();
    let inputs = fixture(temp.path());
    let output = temp.path().join("output");
    let result = cli(&inputs, &output, "runtime", "exit 42");
    assert_eq!(result.code, 1);
    assert!(!output.exists());
}

// 기존 Python unittest fixture와 실제 wrapper main에서 관측한다. native로 expected를 만들지 않는다.
const ORIGINAL: &str = r#"
import json, pathlib, runpy, sys, types
from unittest.mock import patch
repository, work = map(pathlib.Path, sys.argv[1:])
namespace = runpy.run_path(str(repository / 'rust/tools/tests/test_plannerd_native_memory_controller.py'))
observations = []
for mode in ('success', 'failure', 'empty', 'incomplete', 'unsanitized'):
    directory = work / mode
    directory.mkdir()
    case = namespace['NativeMemoryControllerTests']()
    with patch.object(namespace['tempfile'], 'TemporaryDirectory', lambda **kw: types.SimpleNamespace(name=str(directory), cleanup=lambda: None)):
        case.setUp()
    try:
        if mode == 'unsanitized':
            (case.artifact / 'provenance.json').write_text(json.dumps({'sanitizers': False}))
        result, error, code = None, None, None
        try:
            result = case.invoke('success' if mode == 'unsanitized' else mode)
        except (SystemExit, AssertionError, FileNotFoundError) as exception:
            error = type(exception).__name__
            code = exception.code if isinstance(exception, SystemExit) else None
        wrappers = case.output / 'wrappers/examples'
        observations.append({'case': mode, 'result': result, 'error': error, 'exit_code': code,
            'calls': json.loads(case.calls.read_text()) if case.calls.exists() else None,
            'wrappers': {p.name: p.read_text() for p in wrappers.iterdir()} if wrappers.exists() else {}})
    finally:
        case.doCleanups()
print(json.dumps(observations))
"#;

#[test]
#[ignore = "Actions 원본 memory wrapper 5경계 capture/parity; PLANNER_REFERENCE_PYTHON 및 기존 interpreter proof 필요"]
fn source_memory_wrapper_five_boundaries_match_native_and_capture_original_results() {
    let python = PathBuf::from(
        std::env::var_os("PLANNER_REFERENCE_PYTHON").expect("admitted original Python required"),
    );
    let job = PathBuf::from(std::env::var_os("CI_JOB_TEMP").expect("owned CI job required"));
    let proof = job.join("proof");
    let read_proof = |name: &str| {
        let path = proof.join(name);
        assert!(fs::metadata(&path).unwrap().len() <= 4096);
        fs::read_to_string(path).unwrap()
    };
    let python_hash = digest(&python).unwrap();
    let hash_proof = read_proof("planner-oracle-python.sha256");
    let version_proof = read_proof("planner-oracle-python.version");
    assert_eq!(
        hash_proof.trim_end(),
        format!("{python_hash}  {}", python.display())
    );
    assert!(version_proof.trim().starts_with("Python "));
    let github_sha = std::env::var("GITHUB_SHA").unwrap();
    assert!(github_sha.len() == 40 && github_sha.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let paths = [
        "rust/tools/check_plannerd_native_memory.py",
        "rust/tools/tests/test_plannerd_native_memory_controller.py",
        "rust/crates/dev-tools/src/planner_ci_controller.rs",
        "rust/crates/dev-tools/src/planner_ci_controller/cli.rs",
        "rust/crates/dev-tools/src/planner_ci_controller/memory.rs",
        "rust/crates/dev-tools/src/planner_ci_controller/plan.rs",
        "rust/crates/dev-tools/src/bin/check_plannerd_ci.rs",
        "rust/crates/dev-tools/src/qa_process.rs",
        "rust/crates/dev-tools/src/seal.rs",
        "rust/crates/dev-tools/src/path_resolve.rs",
        "rust/crates/disk-space/src/lib.rs",
        "rust/crates/dev-tools/tests/planner_memory_controller.rs",
        "tools/ci/planner_native_controller_523.sh",
    ];
    let hashes = || {
        paths
            .iter()
            .map(|path| ((*path).to_owned(), digest(&root().join(path)).unwrap()))
            .collect::<BTreeMap<_, _>>()
    };
    let before = hashes();
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("original memory ' 한글");
    fs::create_dir(&work).unwrap();
    let mut command = CommandSpec::new(
        vec![
            python.to_str().unwrap().into(),
            "-I".into(),
            "-B".into(),
            "-S".into(),
            "-c".into(),
            ORIGINAL.into(),
            root().to_str().unwrap().into(),
            work.to_str().unwrap().into(),
        ],
        Duration::from_secs(30),
    );
    command.stdout_limit = 1024 * 1024;
    command.stderr_limit = 65536;
    let original = NativeRunner.run(&command).unwrap();
    assert_eq!(original.code, 0, "{original:?}");
    let observations: Value = serde_json::from_slice(&original.stdout).unwrap();
    assert_eq!(observations.as_array().unwrap().len(), 5);
    assert_eq!(hashes(), before);
    assert_eq!(digest(&python).unwrap(), python_hash);
    assert_eq!(read_proof("planner-oracle-python.sha256"), hash_proof);
    assert_eq!(read_proof("planner-oracle-python.version"), version_proof);
    // native 비교 전에 원본을 남겨 parity 실패 때도 독립 actual 결과를 보존한다.
    let capture = json!({"schema_version":1,"kind":"independent-python-memory-observation","github_sha":github_sha,"source_sha256":before,
        "python":{"executable":python,"executable_sha256":python_hash,"sha256_proof":hash_proof,"version_proof":version_proof},
        "cases":["success","failure","empty","incomplete","unsanitized"],"argv":command.argv,
        "original_returncode":original.code,"original_stdout_utf8":std::str::from_utf8(&original.stdout).unwrap(),"original_stderr_utf8":std::str::from_utf8(&original.stderr).unwrap(),
        "original_results":observations,"native_comparison":"following parity assertions must pass; capture alone is not acceptance"});
    let mut bytes = serde_json::to_vec_pretty(&capture).unwrap();
    bytes.push(b'\n');
    assert!(bytes.len() <= 1024 * 1024);
    let directory = proof.join("planner-memory-original");
    fs::create_dir_all(&directory).unwrap();
    assert_eq!(fs::canonicalize(&directory).unwrap(), directory);
    assert!(fs::symlink_metadata(&directory)
        .unwrap()
        .file_type()
        .is_dir());
    assert!(fs::read_dir(&directory).unwrap().next().is_none());
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join("observations.json"))
        .unwrap();
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
    for row in observations.as_array().unwrap() {
        let mode = row["case"].as_str().unwrap();
        let directory = work.join(mode);
        let mut inputs = inputs(&directory);
        let output = directory.join("output");
        let asan = directory.join("libasan.so");
        let mut leaves = Leaves::default();
        match mode {
            "success" | "failure" => {
                assert!(row["calls"].as_array().is_some());
                let option = |name: &str| {
                    let args = row["calls"].as_array().unwrap();
                    args[args.iter().position(|value| value == name).unwrap() + 1]
                        .as_str()
                        .unwrap()
                };
                assert_eq!(
                    option("--bin-dir"),
                    output.join("wrappers").to_str().unwrap()
                );
                assert_eq!(
                    option("--output"),
                    output.join("comparison").to_str().unwrap()
                );
                assert_eq!(option("--timeout-seconds"), "2700");
                inputs.python = option("--python").into();
                assert_eq!(option("--root"), inputs.root.to_str().unwrap());
                assert_eq!(option("--artifact"), inputs.artifact.to_str().unwrap());
                assert_eq!(option("--source-native"), inputs.source.to_str().unwrap());
                assert_eq!(option("--binding"), inputs.binding.to_str().unwrap());
                let args = row["calls"].as_array().unwrap();
                let budgets = args
                    .windows(2)
                    .filter(|pair| pair[0] == "--growth-mib")
                    .map(|pair| pair[1].as_str().unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(budgets, ["abi=128", "owner=1024", "fault=1024"]);
                fs::remove_dir_all(&output).unwrap();
                if mode == "failure" {
                    leaves.fail_at = Some(0);
                }
                let result = memory::run(
                    &inputs,
                    &asan,
                    &output,
                    &growth(),
                    Duration::from_secs(2700),
                    &mut leaves,
                );
                if mode == "success" {
                    assert_eq!(result.unwrap(), row["result"]);
                    for (name, body) in row["wrappers"].as_object().unwrap() {
                        assert_eq!(
                            fs::read_to_string(output.join("wrappers/examples").join(name))
                                .unwrap(),
                            body.as_str().unwrap()
                        );
                    }
                    assert!(!output.join("comparison/result.json").exists());
                } else {
                    assert_eq!(row["exit_code"], 42);
                    assert_eq!(
                        result
                            .unwrap_err()
                            .downcast_ref::<CheckFailure>()
                            .unwrap()
                            .returncode,
                        42
                    );
                    assert_eq!(leaves.calls.len(), 1);
                    assert!(!output.join("result.json").exists());
                }
            }
            "empty" | "incomplete" => {
                assert!(row["error"].is_string());
                assert!(
                    memory::write_receipt(&inputs.artifact, &asan, &output, &BTreeMap::new())
                        .is_err()
                );
                assert!(!output.join("result.json").exists());
            }
            "unsanitized" => {
                assert_eq!(row["error"], "AssertionError");
                assert!(row["calls"].is_null());
                assert!(memory::run(
                    &inputs,
                    &asan,
                    &output,
                    &growth(),
                    Duration::from_secs(2700),
                    &mut leaves
                )
                .is_err());
                assert!(leaves.calls.is_empty());
                assert!(!output.exists());
            }
            _ => panic!("unknown original memory case"),
        }
    }
    assert_eq!(hashes(), before);
    assert_eq!(digest(&python).unwrap(), python_hash);
    assert_eq!(read_proof("planner-oracle-python.sha256"), hash_proof);
    assert_eq!(read_proof("planner-oracle-python.version"), version_proof);
}
