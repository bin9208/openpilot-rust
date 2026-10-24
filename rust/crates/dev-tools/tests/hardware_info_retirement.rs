//! Hosted same-checker replay against the original and retired-source trees.
#![cfg(all(target_os = "linux", feature = "hardware-info-qa-native"))]

use openpilot_dev_tools::{
    hardware_info_capture::{self, Receipt},
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::Duration,
};

const RETIRED: [&str; 3] = [
    "rust/tools/check_hardware_info.py",
    "rust/tools/hardware_info_cases.py",
    "rust/tools/hardware_info_reference.py",
];
const SOURCE_COUNT: usize = 27;

fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} required"))
        .into()
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn source_pins(capture: &Path) -> BTreeMap<String, String> {
    let manifest: Value =
        serde_json::from_slice(&fs::read(capture.join("capture-manifest.json")).unwrap()).unwrap();
    manifest["source_sha256"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, hash)| (name.clone(), hash.as_str().unwrap().to_owned()))
        .collect()
}

fn make_private_root(
    source_root: &Path,
    capture: &Path,
    destination: &Path,
    require_retired: bool,
) -> Value {
    fs::create_dir(destination).unwrap();
    let pins = source_pins(capture);
    assert_eq!(pins.len(), SOURCE_COUNT + RETIRED.len());
    let mut copied = Vec::new();
    for (name, expected) in pins {
        let src = source_root.join(&name);
        if RETIRED.contains(&name.as_str()) {
            if require_retired {
                assert!(
                    fs::symlink_metadata(&src)
                        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                    "retired source still exists: {name}"
                );
            }
            continue;
        }
        let metadata = fs::symlink_metadata(&src).unwrap();
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        let bytes = fs::read(&src).unwrap();
        assert_eq!(sha(&bytes), expected, "source pin differs: {name}");
        let dst = destination.join(&name);
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        fs::write(&dst, &bytes).unwrap();
        let copied_bytes = fs::read(&dst).unwrap();
        assert_eq!(copied_bytes.len(), bytes.len());
        assert_eq!(sha(&copied_bytes), expected);
        copied.push(json!({"path":name,"bytes":bytes.len(),"sha256":expected}));
    }
    assert_eq!(copied.len(), SOURCE_COUNT);
    for name in RETIRED {
        assert!(fs::symlink_metadata(destination.join(name)).is_err());
    }
    json!({"source_count":copied.len(),"sources":copied,"retired_absent":RETIRED})
}

fn args_for(
    binary: &Path,
    plans: &Path,
    capture: &Path,
    output: &Path,
    root: &Path,
    retired: bool,
) -> Vec<OsString> {
    let mut args = vec![
        binary.as_os_str().to_owned(),
        plans.as_os_str().to_owned(),
        capture.as_os_str().to_owned(),
        output.as_os_str().to_owned(),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
    ];
    if retired {
        args.push("--require-retired".into());
    }
    args
}

fn run_checker(
    checker: &Path,
    args: &[OsString],
    cwd: &Path,
    evidence: &Path,
    name: &str,
) -> Captured {
    fs::create_dir(evidence).unwrap();
    let argv = std::iter::once(checker.as_os_str().to_owned())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(20 * 60));
    spec.cwd = Some(cwd.to_path_buf());
    spec.stdout_limit = 2 * 1024 * 1024;
    spec.stderr_limit = 2 * 1024 * 1024;
    spec.env_remove = vec!["PYTHONPATH".into(), "PYTHONHOME".into()];
    let result = NativeRunner.run_to_files(
        &spec,
        &evidence.join("stdout.log"),
        &evidence.join("stderr.log"),
    );
    let stdout = fs::read(evidence.join("stdout.log")).unwrap();
    let stderr = fs::read(evidence.join("stderr.log")).unwrap();
    let status = result.as_ref().ok().map(|captured| captured.code);
    write_json(
        &evidence.join("invocation.json"),
        &json!({
            "name":name,
            "argv_bytes":argv.iter().map(|arg| arg.as_bytes().to_vec()).collect::<Vec<_>>(),
            "cwd":cwd,
            "timeout_seconds":spec.timeout.as_secs(),
            "stdout_bytes":stdout.len(),
            "stdout_sha256":sha(&stdout),
            "stderr_bytes":stderr.len(),
            "stderr_sha256":sha(&stderr),
            "exit_code":status,
            "error":result.as_ref().err().map(ToString::to_string)
        }),
    );
    let result = result.unwrap_or_else(|error| panic!("{name}: {error}"));
    Captured {
        code: result.code,
        stdout,
        stderr,
    }
}

fn validate(
    root: &Path,
    capture: &Path,
    plans: &Path,
    require_retired: bool,
    binary: &Path,
    runner: &[OsString],
) -> Receipt {
    hardware_info_capture::validate_for_runner(
        root,
        capture,
        plans,
        require_retired,
        binary,
        runner,
    )
    .unwrap()
}

fn cli_control(
    checker: &Path,
    args: &[OsString],
    cwd: &Path,
    evidence: &Path,
    name: &str,
    expected: i32,
) -> Value {
    let result = run_checker(checker, args, cwd, evidence, name);
    assert_eq!(
        result.code,
        expected,
        "{name}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    json!({"name":name,"expected_status":expected,"status":result.code,
        "stdout_sha256":sha(&result.stdout),"stderr_sha256":sha(&result.stderr),
        "stdout_bytes":result.stdout.len(),"stderr_bytes":result.stderr.len()})
}

#[test]
#[ignore = "hosted post-retirement original corpus, source checkout, and current-host ELF required"]
fn same_elf_checks_all_original_cases_in_actual_and_private_roots() {
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-hardware-info"))
        .canonicalize()
        .unwrap();
    let binary = required("HARDWARE_INFO_BINARY").canonicalize().unwrap();
    let capture = required("HARDWARE_INFO_EVIDENCE").canonicalize().unwrap();
    let plans = capture.join("plans.json.gz");
    let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let proof = required("HARDWARE_INFO_OUTPUT");
    fs::create_dir(&proof).unwrap();
    let actual_requires_retirement =
        std::env::var_os("HARDWARE_INFO_REQUIRE_RETIRED").is_some_and(|value| value == "1");
    let actual_receipt = validate(
        &source_root,
        &capture,
        &plans,
        actual_requires_retirement,
        &binary,
        &[],
    );
    let actual_checker_receipt = validate(
        &source_root,
        &capture,
        &plans,
        actual_requires_retirement,
        &checker,
        &[],
    );
    let checker_sha = sha(&fs::read(&checker).unwrap());
    let binary_sha = sha(&fs::read(&binary).unwrap());
    let empty_cwd = proof.join("empty-cwd");
    fs::create_dir(&empty_cwd).unwrap();
    let private_root = proof.join("private-root");
    let private_copy = make_private_root(
        &source_root,
        &capture,
        &private_root,
        actual_requires_retirement,
    );
    let private_receipt = validate(&private_root, &capture, &plans, true, &binary, &[]);
    let private_checker_receipt = validate(&private_root, &capture, &plans, true, &checker, &[]);

    let mut reports = Vec::new();
    for (name, root, require_retired, receipt) in [
        (
            "actual",
            &source_root,
            actual_requires_retirement,
            &actual_receipt,
        ),
        ("private", &private_root, true, &private_receipt),
    ] {
        receipt.verify().unwrap();
        let checker_receipt = if name == "actual" {
            &actual_checker_receipt
        } else {
            &private_checker_receipt
        };
        checker_receipt.verify().unwrap();
        let before = receipt.report();
        let checker_before = checker_receipt.report();
        let output = proof.join(format!("{name}-output"));
        let args = args_for(&binary, &plans, &capture, &output, root, require_retired);
        let result = run_checker(
            &checker,
            &args,
            if name == "actual" {
                &source_root
            } else {
                &empty_cwd
            },
            &proof.join(format!("{name}-run")),
            name,
        );
        assert_eq!(
            result.code,
            0,
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value =
            serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(report["result"], "PASS");
        assert_eq!(report["scenario_count"], 136);
        assert_eq!(report["comparisons"], 3110);
        assert_eq!(report["datagram_requests_per_implementation"], 70);
        assert_eq!(report["scenarios"].as_array().unwrap().len(), 136);
        receipt.verify().unwrap();
        checker_receipt.verify().unwrap();
        reports.push(json!({"root":name,"report":report,
            "outer_input_receipt_before":before,
            "outer_input_receipt_after":receipt.report(),
            "checker_receipt_before":checker_before,
            "checker_receipt_after":checker_receipt.report()}));
    }

    // Exercise the configured ELF runner protocol on an independently selected
    // three-case slice. This confirms argv preservation, not foreign execution.
    let runner = vec![
        OsString::from("/usr/bin/env"),
        OsString::from("-u"),
        OsString::from("OPENPILOT_PREFIX"),
    ];
    let runner_receipt = validate(&private_root, &capture, &plans, true, &binary, &runner);
    let runner_output = proof.join("runner-output");
    let mut runner_args = args_for(
        &binary,
        &plans,
        &capture,
        &runner_output,
        &private_root,
        true,
    );
    runner_args.extend([
        OsString::from("--cases"),
        OsString::from("^missing-(base|pc|tici)$"),
        OsString::from("--runner"),
    ]);
    runner_args.extend(runner.iter().cloned());
    let runner_result = run_checker(
        &checker,
        &runner_args,
        &empty_cwd,
        &proof.join("runner-protocol"),
        "configured-runner-three-cases",
    );
    assert_eq!(
        runner_result.code,
        0,
        "{}",
        String::from_utf8_lossy(&runner_result.stderr)
    );
    let runner_report: Value =
        serde_json::from_slice(&fs::read(runner_output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(runner_report["scenario_count"], 3);
    assert_eq!(
        runner_receipt.report()["runner_argv_bytes"]
            .as_array()
            .unwrap()
            .len(),
        runner.len()
    );
    runner_receipt.verify().unwrap();

    let baseline_args = args_for(
        &binary,
        &plans,
        &capture,
        &proof.join("negative-placeholder"),
        &source_root,
        false,
    );
    let cli = vec![
        ("help", vec![OsString::from("--help")], 0),
        ("missing-arguments", Vec::new(), 2),
        (
            "unknown-option",
            vec![OsString::from("--not-a-real-option")],
            2,
        ),
    ];
    let mut controls = Vec::new();
    for (name, args, expected) in cli {
        controls.push(cli_control(
            &checker,
            &args,
            &empty_cwd,
            &proof.join(format!("cli-{name}")),
            name,
            expected,
        ));
    }
    let file_output = proof.join("output-is-file");
    fs::write(&file_output, b"occupied output path").unwrap();
    let mut args = baseline_args.clone();
    args[3] = file_output.into_os_string();
    controls.push(cli_control(
        &checker,
        &args,
        &empty_cwd,
        &proof.join("cli-output-file"),
        "output-file",
        1,
    ));
    let mut unmatched = baseline_args.clone();
    unmatched[3] = proof.join("unmatched-output").into_os_string();
    unmatched.extend([
        OsString::from("--cases"),
        OsString::from("^no-such-hardware-info-case$"),
    ]);
    controls.push(cli_control(
        &checker,
        &unmatched,
        &empty_cwd,
        &proof.join("cli-unmatched-filter"),
        "unmatched-filter",
        1,
    ));
    let mut bad_regex = baseline_args;
    bad_regex[3] = proof.join("bad-regex-output").into_os_string();
    bad_regex.extend([OsString::from("--cases"), OsString::from("[")]);
    controls.push(cli_control(
        &checker,
        &bad_regex,
        &empty_cwd,
        &proof.join("cli-bad-regex"),
        "bad-regex",
        1,
    ));

    actual_receipt.verify().unwrap();
    actual_checker_receipt.verify().unwrap();
    private_receipt.verify().unwrap();
    private_checker_receipt.verify().unwrap();
    runner_receipt.verify().unwrap();
    assert_eq!(sha(&fs::read(&checker).unwrap()), checker_sha);
    assert_eq!(sha(&fs::read(&binary).unwrap()), binary_sha);
    write_json(
        &proof.join("accepted.json"),
        &json!({"same_elf":true,"checker_sha256":checker_sha,"hardware_info_binary_sha256":binary_sha,
            "source_root":source_root,"capture":capture,"plans":plans,"private_source_copy":private_copy,
            "runs":reports,"runner_case_report":runner_report,"cli_controls":controls,
            "actual_require_retired":actual_requires_retirement,"foreign_execution_claim":false}),
    );
}
