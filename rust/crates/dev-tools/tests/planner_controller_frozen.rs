//! source73의 독립 원본 관측을 데이터로만 소비한다. Python 실행/import는 없다.
use openpilot_dev_tools::{
    planner_ci_controller::{
        plan::{self, Inputs, NativeIpcInputs, NativeLifecycleInputs},
        run_checks, scheduler, Check, CheckFailure,
    },
    qa_process::{CommandSpec, NativeRunner},
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Cursor, path::PathBuf, time::Duration};

const SOURCE: &str = "73e17ff10ec9be7a7001ab9cbf415a779c436f78";
const PYTHON: &str = "/usr/bin/python3.12";
const PYTHON_SHA: &str = "e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f";
const SOURCE_PINS: [(&str, &str); 10] = [
    (
        "rust/tools/check_plannerd_ci.py",
        "dda14614fe042939bd6758cffda69da95dc141ca8ce4b0c7e67d046ebaff2157",
    ),
    (
        "rust/tools/plannerd_ipc.py",
        "c2099127901a2883925305dc3bf0c563b338494cd5bd829cb7bd3e916ffdd031",
    ),
    (
        "rust/crates/dev-tools/tests/planner_ci_controller.rs",
        "a5e38fcfff88611926b04c334988f05faaf63754362b6d4989d4771668a3493f",
    ),
    (
        "rust/crates/dev-tools/src/planner_ci_controller.rs",
        "c5b738aaba3f219307611d2bdd6efee92c341487a0463e5a9d484394af069b62",
    ),
    (
        "rust/crates/dev-tools/src/planner_ci_controller/cli.rs",
        "f336e849a7554ea154df1c55ee4f721a894eafd1585075ae97556d6cbe8bc87e",
    ),
    (
        "rust/crates/dev-tools/src/planner_ci_controller/plan.rs",
        "c1dddd235346b8f415c9b131aba7a371cb67ae80d0db5f13a86b450c4d0a8b44",
    ),
    (
        "rust/crates/dev-tools/src/planner_ci_controller/scheduler.rs",
        "a35ad7f6c4eab2348ef3ceae318e8bd37980e251e84e59d5d17480e7b9d55f0a",
    ),
    (
        "rust/crates/dev-tools/src/bin/check_plannerd_ci.rs",
        "32d2b777eec4ff960ea70195aa0d7b6ac553aee8621658462033ddcf933985d9",
    ),
    (
        "rust/crates/dev-tools/src/qa_process.rs",
        "aa9fda1ed867fe6dcf7befb19fc6857fa419bbaa33fcc518264276abef7c6633",
    ),
    (
        "rust/crates/disk-space/src/lib.rs",
        "fa920db8d68ffbe2772c8c60a7e793346427b5d9add0a81184d3fa162ea34bd0",
    ),
];

struct Fixture {
    raw: &'static [u8],
    sha256: &'static str,
    name: &'static str,
}

const FINITE: Fixture = Fixture {
    raw: include_bytes!("fixtures/planner-controller-original/scheduler-finite.json"),
    sha256: "0542466a70770f8a04c9c81893ecfe4c3252daf00b2935eeba5c3c6a4305a7bb",
    name: "source_scheduler_matches_native_protocol_without_reimplementing_the_oracle",
};
const IEEE: Fixture = Fixture {
    raw: include_bytes!("fixtures/planner-controller-original/scheduler-ieee.json"),
    sha256: "5ef56d3d67666a172cbc18ee71229d7a6f31d9791bf7a1944f3fa588a64bc53a",
    name: "source_public_scheduler_preserves_python_ordered_comparison_at_float_boundaries",
};
const RUNTIME: Fixture = Fixture {
    raw: include_bytes!("fixtures/planner-controller-original/runtime-plan.json"),
    sha256: "fe1d7e233882afda6214627e090f702f8498c0222ec39bc5e577b53adbdf6ebd",
    name: "source_runtime_plan_matches_every_native_leaf_and_argument",
};
const MEMORY: Fixture = Fixture {
    raw: include_bytes!("fixtures/planner-controller-original/memory-plan.json"),
    sha256: "b73af7d13a5d8dd2e7522d2e016bc21300d884792c16dd7ab2028539bb60d9ce",
    name: "source_memory_subset_matches_every_native_sanitizer_leaf_argument",
};
const FAILURE: Fixture = Fixture {
    raw: include_bytes!("fixtures/planner-controller-original/failure-forward.json"),
    sha256: "dddc951fd972043d0170e9ad3f90988245099974f594d2f7e52b61ea0aeccd3f",
    name: "source_failure_forward_matches_native_controller_and_raw_log",
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PythonProof {
    executable: String,
    executable_sha256: String,
    sha256_proof: String,
    version_proof: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    schema_version: u32,
    kind: String,
    test_name: String,
    github_sha: String,
    source_sha256: BTreeMap<String, String>,
    python: PythonProof,
    cases: Value,
    argv: Vec<String>,
    stdin_utf8: Option<String>,
    original_result: Value,
    original_stdout_utf8: String,
    original_stderr_utf8: String,
    original_returncode: i32,
    original_first_log_utf8: Option<String>,
    native_comparison: String,
}

impl Fixture {
    fn read(&self) -> Observation {
        assert_eq!(format!("{:x}", Sha256::digest(self.raw)), self.sha256);
        let data: Observation = serde_json::from_slice(self.raw).unwrap();
        assert_eq!(data.schema_version, 1);
        assert_eq!(data.kind, "independent-python-observation");
        assert_eq!(data.test_name, self.name);
        assert_eq!(data.github_sha, SOURCE);
        let pins = SOURCE_PINS
            .into_iter()
            .map(|(path, sha)| (path.to_owned(), sha.to_owned()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(data.source_sha256, pins);
        assert_eq!(data.python.executable, PYTHON);
        assert_eq!(data.python.executable_sha256, PYTHON_SHA);
        assert_eq!(
            data.python.sha256_proof,
            format!("{PYTHON_SHA}  {PYTHON}\n")
        );
        assert_eq!(data.python.version_proof, "Python 3.12.3\n");
        let scheduler = self.name == FINITE.name || self.name == IEEE.name;
        let failure = self.name == FAILURE.name;
        assert_eq!(
            data.argv.len(),
            if scheduler {
                5
            } else if failure {
                8
            } else {
                6
            }
        );
        assert_eq!(data.argv[0], PYTHON);
        assert_eq!(data.argv[1], "-I");
        assert_eq!(data.argv[2], "-c");
        assert!(!data.argv[3].is_empty());
        assert!(PathBuf::from(&data.argv[4]).is_absolute());
        assert_eq!(data.stdin_utf8.is_some(), scheduler);
        assert_eq!(data.original_first_log_utf8.is_some(), failure);
        assert_eq!(data.original_returncode, 0);
        assert_eq!(data.original_stderr_utf8, "");
        let stdout_result: Value =
            serde_json::from_str(data.original_stdout_utf8.lines().last().unwrap()).unwrap();
        assert_eq!(stdout_result, data.original_result);
        assert_eq!(
            data.native_comparison,
            "enclosing test assertions must pass; observation alone is not acceptance"
        );
        data
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SchedulerCases {
    encoding: String,
    timestamps: Vec<[u64; 3]>,
}

fn scheduler_data(fixture: &Fixture, count: usize) -> (Vec<[f64; 3]>, Vec<u64>) {
    let data = fixture.read();
    let cases: SchedulerCases = serde_json::from_value(data.cases).unwrap();
    assert_eq!(cases.encoding, "ieee754-binary64-bits");
    let expected: Vec<u64> = serde_json::from_value(data.original_result).unwrap();
    assert_eq!(cases.timestamps.len(), count);
    assert_eq!(expected.len(), count);
    (
        cases
            .timestamps
            .into_iter()
            .map(|case| case.map(f64::from_bits))
            .collect(),
        expected,
    )
}

#[test]
fn frozen_original_finite76_matches_native_scheduler_wire_bits() {
    let (cases, expected) = scheduler_data(&FINITE, 76);
    assert!(cases.iter().flatten().all(|value| value.is_finite()));
    let request = cases
        .iter()
        .map(|[nominal, sent, done]| format!("{nominal:?}\t{sent:?}\t{done:?}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    scheduler::serve(Cursor::new(request), &mut output).unwrap();
    let actual = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| line.parse::<f64>().unwrap().to_bits())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn frozen_original_ieee9_matches_native_public_scheduler_bits() {
    let (cases, expected) = scheduler_data(&IEEE, 9);
    let actual = cases
        .into_iter()
        .map(|[nominal, sent, done]| scheduler::next_model_target(nominal, sent, done).to_bits())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanCases {
    binaries: PathBuf,
    artifact: PathBuf,
    source: PathBuf,
    binding: PathBuf,
    output: PathBuf,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Leaf {
    name: String,
    argv: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalPlan {
    python: String,
    checks: Vec<Leaf>,
}

// PROVENANCE.md의 정확한 7개 치환만 허용한다. 임의 temp/root 제거는 하지 않는다.
struct PathMapping {
    original: PathBuf,
    native: PathBuf,
    directory: bool,
}

fn mapped_argument(argument: &str, mappings: &[PathMapping]) -> String {
    let path = PathBuf::from(argument);
    if !path.is_absolute() {
        return argument.to_owned();
    }
    let matches = mappings
        .iter()
        .filter_map(|mapping| {
            assert!(mapping.original.is_absolute() && mapping.native.is_absolute());
            if path == mapping.original {
                Some(mapping.native.clone())
            } else if mapping.directory {
                path.strip_prefix(&mapping.original)
                    .ok()
                    .map(|suffix| mapping.native.join(suffix))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "unmapped/ambiguous frozen argument: {argument}"
    );
    matches[0].to_str().unwrap().to_owned()
}

fn compare_plan(fixture: &Fixture, memory: bool, count: usize) {
    let data = fixture.read();
    let original_cases: PlanCases = serde_json::from_value(data.cases).unwrap();
    let recorded: OriginalPlan = serde_json::from_value(data.original_result).unwrap();
    assert_eq!(recorded.python, PYTHON);
    assert_eq!(recorded.checks.len(), count);
    let base = PathBuf::from(if memory {
        "/planner frozen/memory"
    } else {
        "/planner frozen/runtime"
    });
    let inputs = Inputs {
        root: base.join("repository"),
        python: base.join("python"),
        binaries: base.join(if memory { "wrappers" } else { "bin" }),
        artifact: base.join("artifact"),
        source: base.join("source"),
        binding: base.join("params.so"),
        scheduler: base.join("native-scheduler"),
        native_ipc: (!memory).then(|| NativeIpcInputs {
            original_ipc_binding: base.join("original/ipc_pyx.fixture.so"),
            compiler: base.join("compiler"),
            corpus: base.join("corpus"),
            corpus_audit: base.join("corpus/audit.json"),
            corpus_audit_sha256: "b".repeat(64),
            timeout: Duration::from_secs(3),
            growth_mib: 512,
            peer_build_growth_mib: 128,
        }),
        native_lifecycle: (!memory).then(|| NativeLifecycleInputs {
            original_ipc_binding: base.join("original/ipc_pyx.fixture.so"),
            corpus: base.join("lifecycle-corpus"),
            corpus_audit: base.join("lifecycle-corpus/audit.json"),
            growth_mib: 256,
        }),
    };
    let output = base.join("output");
    let mappings = [
        PathMapping {
            original: data.argv[4].clone().into(),
            native: inputs.root.clone(),
            directory: true,
        },
        PathMapping {
            original: data.python.executable.into(),
            native: inputs.python.clone(),
            directory: false,
        },
        PathMapping {
            original: original_cases.binaries,
            native: inputs.binaries.clone(),
            directory: true,
        },
        PathMapping {
            original: original_cases.artifact,
            native: inputs.artifact.clone(),
            directory: true,
        },
        PathMapping {
            original: original_cases.source,
            native: inputs.source.clone(),
            directory: true,
        },
        PathMapping {
            original: original_cases.binding,
            native: inputs.binding.clone(),
            directory: false,
        },
        PathMapping {
            original: original_cases.output,
            native: output.clone(),
            directory: true,
        },
    ];
    let growth = recorded
        .checks
        .iter()
        .map(|check| (check.name.clone(), 0))
        .collect();
    let timeout = Duration::from_secs(5);
    let checks = if memory {
        plan::memory_checks(&inputs, &growth, timeout)
    } else {
        plan::runtime_checks(&inputs, &growth, timeout)
    }
    .unwrap();
    let actual = checks
        .iter()
        .map(|check| Leaf {
            name: check.name.clone(),
            argv: check
                .invocation(&output, &inputs.root)
                .unwrap()
                .effective_argv()
                .into_iter()
                .map(|arg| arg.into_string().unwrap())
                .collect(),
        })
        .collect::<Vec<_>>();
    let expected = recorded
        .checks
        .into_iter()
        .map(|check| Leaf {
            name: check.name,
            argv: check
                .argv
                .iter()
                .map(|argument| mapped_argument(argument, &mappings))
                .collect(),
        })
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), expected.len());
    let mut ipc_cutovers = 0;
    let mut lifecycle_cutovers = 0;
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        if !memory && index == 10 {
            // source73 원문 IPC argv도 전부 확인한다. 새 native argv로 원문을 덮어쓰지 않는다.
            assert_eq!(expected.name, "ipc");
            assert_eq!(
                expected.argv,
                [
                    inputs.python.clone(),
                    "-u".into(),
                    "-P".into(),
                    inputs.root.join("rust/tools/plannerd_ipc.py"),
                    "--source-native".into(),
                    inputs.source.clone(),
                    "--artifact".into(),
                    inputs.artifact.clone(),
                    "--python".into(),
                    inputs.python.clone(),
                    "--binding".into(),
                    inputs.binding.clone(),
                    "--binary".into(),
                    inputs.binaries.join("openpilot-plannerd"),
                    "--output".into(),
                    output.join("ipc"),
                ]
                .map(|path| path.to_str().unwrap().to_owned())
            );
            assert_eq!(
                actual,
                &Leaf {
                    name: "ipc".into(),
                    argv: [
                        inputs.scheduler.clone(),
                        "ipc".into(),
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
                        base.join("original/ipc_pyx.fixture.so"),
                        "--scheduler".into(),
                        inputs.scheduler.clone(),
                        "--compiler".into(),
                        base.join("compiler"),
                        "--corpus".into(),
                        base.join("corpus"),
                        "--corpus-audit".into(),
                        base.join("corpus/audit.json"),
                        "--corpus-audit-sha256".into(),
                        "b".repeat(64).into(),
                        "--timeout-seconds".into(),
                        "3".into(),
                        "--growth-mib".into(),
                        "512".into(),
                        "--peer-build-growth-mib".into(),
                        "128".into(),
                        "--output".into(),
                        output.join("ipc"),
                        "--evidence".into(),
                        output.join("ipc-scheduler.tsv"),
                    ]
                    .map(|path| path.to_str().unwrap().to_owned())
                    .into(),
                }
            );
            ipc_cutovers += 1;
        } else if !memory && index == 11 {
            // source73 원래 lifecycle argv/순서를 먼저 확인한다. 동결 원문을 새 expected로 덮어쓰지 않는다.
            assert_eq!(expected.name, "lifecycle");
            assert_eq!(
                expected.argv,
                [
                    inputs.python.clone(),
                    "-u".into(),
                    "-P".into(),
                    inputs.root.join("rust/tools/check_plannerd_lifecycle.py"),
                    "--source-native".into(),
                    inputs.source.clone(),
                    "--artifact".into(),
                    inputs.artifact.clone(),
                    "--python".into(),
                    inputs.python.clone(),
                    "--binding".into(),
                    inputs.binding.clone(),
                    "--binary".into(),
                    inputs.binaries.join("openpilot-plannerd"),
                    "--output".into(),
                    output.join("lifecycle")
                ]
                .map(|path| path.to_str().unwrap().to_owned())
            );
            assert_eq!(
                actual,
                &Leaf {
                    name: "lifecycle".into(),
                    argv: [
                        inputs.scheduler.clone(),
                        "lifecycle".into(),
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
                        base.join("original/ipc_pyx.fixture.so"),
                        "--corpus".into(),
                        base.join("lifecycle-corpus"),
                        "--corpus-audit".into(),
                        base.join("lifecycle-corpus/audit.json"),
                        "--timeout-seconds".into(),
                        "5".into(),
                        "--growth-mib".into(),
                        "256".into(),
                        "--output".into(),
                        output.join("lifecycle")
                    ]
                    .map(|path| path.to_str().unwrap().to_owned())
                    .into()
                }
            );
            lifecycle_cutovers += 1;
        } else {
            // runtime 나머지10개와 memory3는 순서와 전체 argv를 원문 그대로 대조한다.
            assert_eq!(actual, expected, "frozen leaf at index {index}");
        }
    }
    assert_eq!(ipc_cutovers, usize::from(!memory));
    assert_eq!(lifecycle_cutovers, usize::from(!memory));
}

#[test]
fn frozen_original_runtime12_matches_complete_native_names_and_argv() {
    compare_plan(&RUNTIME, false, 12);
}

#[test]
fn frozen_original_memory3_matches_complete_native_names_and_argv() {
    compare_plan(&MEMORY, true, 3);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureCases {
    disk_usage_free_override: u64,
    failing_path: PathBuf,
    failing_script: String,
    later_path: PathBuf,
    later_script: String,
    sentinel_path: PathBuf,
    source_output: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureRecord {
    argv: Vec<String>,
    free_before: u64,
    name: String,
    returncode: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalFailure {
    python: String,
    returncode: i32,
    rows: Vec<FailureRecord>,
}

#[test]
fn frozen_original_failure42_and_raw_log_match_native_process_fixture() {
    let data = FAILURE.read();
    let cases: FailureCases = serde_json::from_value(data.cases).unwrap();
    let expected: OriginalFailure = serde_json::from_value(data.original_result).unwrap();
    assert_eq!(expected.python, PYTHON);
    assert_eq!(expected.rows.len(), 1);
    let first = &expected.rows[0];
    assert_eq!(first.argv[0], PYTHON);
    assert_eq!(first.free_before, cases.disk_usage_free_override);
    assert!(!cases.failing_script.is_empty() && !cases.later_script.is_empty());
    for path in [
        cases.failing_path,
        cases.later_path,
        cases.sentinel_path,
        cases.source_output,
    ] {
        assert!(path.is_absolute());
    }
    // 기존 qa_process/controller의 /bin/sh 실제 child fixture를 사용한다.
    // 원본 Python script argv와 동일하다고 주장하지 않고 상태·raw log·중단을 비교한다.
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("result");
    let sentinel = temp.path().join("sentinel");
    let check = |name: &str, argv| Check {
        name: name.into(),
        command: CommandSpec::new(argv, Duration::from_secs(5)),
        output_option: "--output".into(),
        growth_bytes: 0,
        scheduler_calls: None,
    };
    let checks = [
        check(
            "first",
            vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf 'out\\n'; printf 'err\\n' >&2; exit 42".into(),
            ],
        ),
        check(
            "later",
            vec![
                "/bin/sh".into(),
                "-c".into(),
                ": > \"$1\"".into(),
                "planner-frozen-sentinel".into(),
                sentinel.to_str().unwrap().into(),
            ],
        ),
    ];
    let error = run_checks(&checks, &output, temp.path(), &mut NativeRunner).unwrap_err();
    let failure = error.downcast_ref::<CheckFailure>().unwrap();
    assert_eq!(failure.returncode, expected.returncode);
    assert_eq!(failure.name, first.name);
    let rows: Value =
        serde_json::from_slice(&fs::read(output.join("commands.json")).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), expected.rows.len());
    assert_eq!(rows[0]["name"].as_str(), Some(first.name.as_str()));
    assert_eq!(
        rows[0]["returncode"].as_i64(),
        Some(i64::from(first.returncode))
    );
    assert!(rows[0]["free_before"].as_u64().unwrap() > 0);
    assert_eq!(
        fs::read(output.join("first.log")).unwrap(),
        data.original_first_log_utf8.unwrap().as_bytes()
    );
    assert!(!sentinel.exists());
    assert!(!output.join("later.log").exists());
    assert!(!output.join("result.json").exists());
}
