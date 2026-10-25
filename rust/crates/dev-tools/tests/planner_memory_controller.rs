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
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

const SCOPE: &str = "generated solver C is instrumented; allocator leak/ownership checks enabled; Rust and pinned external solver libraries are not instrumented";

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

// source425에서 관측한 원문만 읽는다. 안의 Python snippet은 데이터이며 실행하지 않는다.
const FROZEN: &[u8] = include_bytes!("fixtures/planner-memory-original/observations.json");
const FROZEN_SHA: &str = "f2f8c972e65638a1eac1d3eb5ede9114502a48547a403e5f163a2291cd8a0780";
const CAPTURE_BASE: &str = "/tmp/openpilot-ci.4qveQz/.tmpboaa2r";
const CAPTURE_PYTHON: &str = "/usr/bin/python3.12";

fn frozen_case(name: &str) -> Value {
    assert_eq!(FROZEN.len(), 24396);
    assert_eq!(format!("{:x}", Sha256::digest(FROZEN)), FROZEN_SHA);
    let data: Value = serde_json::from_slice(FROZEN).unwrap();
    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["kind"], "independent-python-memory-observation");
    assert_eq!(
        data["github_sha"],
        "425f29d5ffa4ae0e603ceddf5dce8c605e49bc35"
    );
    assert_eq!(data["original_returncode"], 0);
    assert_eq!(data["original_stderr_utf8"], "");
    assert_eq!(data["python"]["executable"], CAPTURE_PYTHON);
    assert_eq!(
        data["python"]["executable_sha256"],
        "e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f"
    );
    assert_eq!(
        data["python"]["sha256_proof"],
        "e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f  /usr/bin/python3.12\n"
    );
    assert_eq!(data["python"]["version_proof"], "Python 3.12.3\n");
    assert_eq!(data["source_sha256"].as_object().unwrap().len(), 13);
    assert_eq!(
        data["source_sha256"]["rust/tools/check_plannerd_native_memory.py"],
        "4f36c97c584d11cccafa53a327893f0d0155f2da0e988ea774d2615396110782"
    );
    assert_eq!(
        data["source_sha256"]["rust/tools/tests/test_plannerd_native_memory_controller.py"],
        "67006fee5e1a81e8350a4b514debd6f7e170af3541afa740c797fe0086016fb4"
    );
    let observed: Value =
        serde_json::from_str(data["original_stdout_utf8"].as_str().unwrap()).unwrap();
    assert_eq!(observed, data["original_results"]);
    let names = ["success", "failure", "empty", "incomplete", "unsanitized"];
    assert_eq!(data["cases"], json!(names));
    let rows = data["original_results"].as_array().unwrap();
    assert_eq!(rows.len(), names.len());
    for (row, name) in rows.iter().zip(names) {
        assert_eq!(row["case"], name);
    }
    rows.iter().find(|row| row["case"] == name).unwrap().clone()
}

fn frozen_fixture(name: &str) -> (tempfile::TempDir, Inputs, Value) {
    // 비교용 임시 접두사는 ASCII다. 관측 당시의 공백/작은따옴표/한글 suffix는 유지한다.
    let temp = tempfile::Builder::new()
        .prefix("planner-memory-frozen-")
        .tempdir_in("/tmp")
        .unwrap();
    assert!(temp
        .path()
        .to_str()
        .unwrap()
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"/._-".contains(&byte)));
    let directory = temp.path().join("original memory ' 한글").join(name);
    fs::create_dir_all(&directory).unwrap();
    let inputs = fixture(&directory);
    let row = frozen_case(name);
    if name != "unsanitized" {
        // 원래 호출의 path component 접두사와 canonical Python exact file만 치환한다.
        let old_root = format!("{CAPTURE_BASE}/original memory ' 한글/{name}");
        let calls = row["calls"].as_array().unwrap();
        assert_eq!(calls.len(), 23);
        let relocated = calls
            .iter()
            .map(|value| {
                let value = value.as_str().unwrap();
                if value == CAPTURE_PYTHON {
                    inputs.python.to_str().unwrap().to_owned()
                } else if value == old_root {
                    directory.to_str().unwrap().to_owned()
                } else if let Some(suffix) = value.strip_prefix(&format!("{old_root}/")) {
                    directory.join(suffix).to_str().unwrap().to_owned()
                } else {
                    assert!(!value.contains(CAPTURE_BASE));
                    value.to_owned()
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            relocated,
            vec![
                "memory".to_owned(),
                "--root".into(),
                inputs.root.to_str().unwrap().into(),
                "--python".into(),
                inputs.python.to_str().unwrap().into(),
                "--bin-dir".into(),
                directory.join("output/wrappers").to_str().unwrap().into(),
                "--artifact".into(),
                inputs.artifact.to_str().unwrap().into(),
                "--source-native".into(),
                inputs.source.to_str().unwrap().into(),
                "--binding".into(),
                inputs.binding.to_str().unwrap().into(),
                "--output".into(),
                directory.join("output/comparison").to_str().unwrap().into(),
                "--timeout-seconds".into(),
                "2700".into(),
                "--growth-mib".into(),
                "abi=128".into(),
                "--growth-mib".into(),
                "owner=1024".into(),
                "--growth-mib".into(),
                "fault=1024".into()
            ]
        );
    }
    (temp, inputs, row)
}

fn frozen_wrappers(temp: &Path, inputs: &Inputs, row: &Value) {
    let name = row["case"].as_str().unwrap();
    let old_prefix = format!("{CAPTURE_BASE}/original memory '\"'\"' 한글/{name}/");
    let new_prefix = format!("{}/original memory '\"'\"' 한글/{name}/", temp.display());
    let wrappers = row["wrappers"].as_object().unwrap();
    assert_eq!(wrappers.len(), 3);
    for (name, body) in wrappers {
        let body = body.as_str().unwrap();
        // 이미 quote된 원문의 두 경로 접두사만 치환한다. 옵션/escape/명령어를 재생성하지 않는다.
        assert_eq!(body.matches(&old_prefix).count(), 2);
        let expected = body.replace(&old_prefix, &new_prefix);
        assert!(!expected.contains(CAPTURE_BASE));
        assert_eq!(
            fs::read_to_string(inputs.root.join("output/wrappers/examples").join(name)).unwrap(),
            expected
        );
    }
}

#[test]
fn frozen_memory_success_matches_original_raw_receipt_and_wrapper_bytes() {
    let (temp, inputs, row) = frozen_fixture("success");
    assert!(row["error"].is_null());
    assert!(row["exit_code"].is_null());
    let mut expected = row["result"].clone();
    let old_root = format!("{CAPTURE_BASE}/original memory ' 한글/success/");
    let identities = expected["binary_sha256"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, hash)| {
            let suffix = path.strip_prefix(&old_root).unwrap();
            (
                inputs.root.join(suffix).to_str().unwrap().to_owned(),
                hash.clone(),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    expected["binary_sha256"] = Value::Object(identities);
    let output = inputs.root.join("output");
    let mut leaves = Leaves::default();
    let receipt = memory::run(
        &inputs,
        &inputs.root.join("libasan.so"),
        &output,
        &growth(),
        Duration::from_secs(2700),
        &mut leaves,
    )
    .unwrap();
    assert_eq!(receipt, expected);
    let file: Value =
        serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
    assert_eq!(file, expected);
    assert_eq!(leaves.calls.len(), 3);
    frozen_wrappers(temp.path(), &inputs, &row);
    assert!(!output.join("comparison/result.json").exists());
}

#[test]
fn frozen_memory_failure_matches_original_42_without_later_leaf_or_pass() {
    let (temp, inputs, row) = frozen_fixture("failure");
    assert_eq!(row["error"], "SystemExit");
    assert_eq!(row["exit_code"], 42);
    assert!(row["result"].is_null());
    let output = inputs.root.join("output");
    let mut leaves = Leaves {
        fail_at: Some(0),
        ..Leaves::default()
    };
    let error = memory::run(
        &inputs,
        &inputs.root.join("libasan.so"),
        &output,
        &growth(),
        Duration::from_secs(2700),
        &mut leaves,
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<CheckFailure>().unwrap().returncode,
        row["exit_code"].as_i64().unwrap() as i32
    );
    assert_eq!(leaves.calls.len(), 1);
    frozen_wrappers(temp.path(), &inputs, &row);
    assert!(!output.join("comparison/owner.log").exists());
    assert!(!output.join("result.json").exists());
}

#[test]
fn frozen_memory_empty_matches_original_missing_record_failure() {
    let (_temp, inputs, row) = frozen_fixture("empty");
    assert_eq!(row["error"], "FileNotFoundError");
    assert!(row["result"].is_null());
    let output = inputs.root.join("output");
    let error = memory::write_receipt(
        &inputs.artifact,
        &inputs.root.join("libasan.so"),
        &output,
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(!output.join("result.json").exists());
}

#[test]
fn frozen_memory_incomplete_matches_original_partial_subset_rejection() {
    let (_temp, inputs, row) = frozen_fixture("incomplete");
    assert_eq!(row["error"], "AssertionError");
    assert!(row["result"].is_null());
    let output = inputs.root.join("output");
    fs::create_dir_all(output.join("comparison")).unwrap();
    // source425의 기존 CONTROLLER fixture가 incomplete에서 쓴 두 행의 고정 입력이다.
    fs::write(
        output.join("comparison/commands.json"),
        br#"[{"name":"abi","returncode":0},{"name":"owner","returncode":0}]"#,
    )
    .unwrap();
    assert!(memory::write_receipt(
        &inputs.artifact,
        &inputs.root.join("libasan.so"),
        &output,
        &BTreeMap::new()
    )
    .is_err());
    assert!(!output.join("result.json").exists());
}

#[test]
fn frozen_memory_unsanitized_matches_original_rejection_before_controller() {
    let (_temp, inputs, row) = frozen_fixture("unsanitized");
    assert_eq!(row["error"], "AssertionError");
    assert!(row["calls"].is_null());
    assert!(row["result"].is_null());
    assert_eq!(row["wrappers"], json!({}));
    fs::write(
        inputs.artifact.join("provenance.json"),
        br#"{"sanitizers":false}"#,
    )
    .unwrap();
    let output = inputs.root.join("output");
    let mut leaves = Leaves::default();
    assert!(memory::run(
        &inputs,
        &inputs.root.join("libasan.so"),
        &output,
        &growth(),
        Duration::from_secs(2700),
        &mut leaves
    )
    .is_err());
    assert!(leaves.calls.is_empty());
    assert!(!output.exists());
}

#[test]
fn memory_absence_guard_rejects_exact_two_reappearances_before_output_or_leaf() {
    for relative in [
        "rust/tools/check_plannerd_native_memory.py",
        "rust/tools/tests/test_plannerd_native_memory_controller.py",
    ] {
        for kind in ["file", "symlink", "dangling-symlink", "directory"] {
            let temp = tempfile::tempdir().unwrap();
            let inputs = fixture(temp.path());
            fs::create_dir_all(inputs.root.join("rust/tools/tests")).unwrap();
            // 다른 원본 controller/IPC는 대상이 아니다. 인접 source의 존재는 허용한다.
            fs::write(
                inputs.root.join("rust/tools/check_plannerd_ci.py"),
                b"independent controller remains",
            )
            .unwrap();
            fs::write(
                inputs.root.join("rust/tools/plannerd_ipc.py"),
                b"independent IPC remains",
            )
            .unwrap();
            let mut before = Leaves::default();
            memory::run(
                &inputs,
                &inputs.root.join("libasan.so"),
                &inputs.root.join("before"),
                &growth(),
                Duration::from_secs(5),
                &mut before,
            )
            .unwrap();
            assert_eq!(before.calls.len(), 3);
            let marker = inputs.root.join(relative);
            match kind {
                "file" => fs::write(&marker, b"retired source reappeared").unwrap(),
                "directory" => fs::create_dir(&marker).unwrap(),
                "symlink" => {
                    std::os::unix::fs::symlink(inputs.artifact.join("provenance.json"), &marker)
                        .unwrap()
                }
                _ => {
                    std::os::unix::fs::symlink(inputs.root.join("absent-target"), &marker).unwrap()
                }
            }
            let output = inputs.root.join("after");
            let mut after = Leaves::default();
            // ASan이 없어도 먼저 source 재등장으로 거부한다. private fixture root만 변경한다.
            let error = memory::run(
                &inputs,
                &inputs.root.join("missing-asan"),
                &output,
                &growth(),
                Duration::from_secs(5),
                &mut after,
            )
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("retired Planner memory source reappeared: {relative}")
            );
            assert!(after.calls.is_empty());
            assert!(!output.exists());
        }
    }
}

#[test]
fn memory_post_guard_preserves_real_leaf_records_and_rejects_restored_sources() {
    for relative in [
        "rust/tools/check_plannerd_native_memory.py",
        "rust/tools/tests/test_plannerd_native_memory_controller.py",
    ] {
        for kind in ["file", "symlink"] {
            for code in [0, 42] {
                let temp = tempfile::tempdir().unwrap();
                let inputs = fixture(temp.path());
                fs::create_dir_all(inputs.root.join("rust/tools/tests")).unwrap();
                let restore = if kind == "file" {
                    format!("printf 'restored source\\n' > \"$root/{relative}\"")
                } else {
                    format!("ln -s \"$root/artifact/provenance.json\" \"$root/{relative}\"")
                };
                let body = format!(
                    "root=$(dirname -- \"$0\")\ncase \"$3\" in */plannerd_acados_source.py) {restore};; esac\nexit {code}"
                );
                let output = inputs.root.join("output");
                let result = cli(&inputs, &output, "memory", &body);
                assert_eq!(result.code, 1, "{result:?}");
                assert!(String::from_utf8(result.stderr).unwrap().contains(&format!(
                    "retired Planner memory source reappeared: {relative}"
                )));
                let records: Value = serde_json::from_slice(
                    &fs::read(output.join("comparison/commands.json")).unwrap(),
                )
                .unwrap();
                assert_eq!(records[0]["name"], "abi");
                assert_eq!(records[0]["returncode"], code);
                assert_eq!(
                    records.as_array().unwrap().len(),
                    if code == 42 { 1 } else { 3 }
                );
                if code == 42 {
                    assert!(!output.join("comparison/owner.log").exists());
                    assert!(!output.join("comparison/fault.log").exists());
                }
                assert!(!output.join("result.json").exists());
                assert!(!output.join("comparison/result.json").exists());
                assert!(fs::symlink_metadata(inputs.root.join(relative)).is_ok());
            }
        }
    }
}
