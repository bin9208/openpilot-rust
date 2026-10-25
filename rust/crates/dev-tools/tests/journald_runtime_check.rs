//! Native real-process journald reference closure and immutable source guards.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
const RETIRED: [&str; 5] = [
    "rust/tools/check_journald.py",
    "rust/tools/journald_peer.py",
    "rust/tools/journal_fixture_input.py",
    "rust/tools/journalctl_fixture.py",
    "rust/tools/tests/test_journal_fixture.py",
];
fn remove_retired(root: &Path, path: &str) {
    assert!(RETIRED.contains(&path));
    match fs::remove_file(root.join(path)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("remove {path}: {e}"),
    }
}
fn copy_source_root(destination: &Path) {
    let root = source_root();
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/journald-check/policy-original.json")).unwrap();
    for path in corpus["sources"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain([
            "rust/crates/dev-tools/src/journald_check.rs",
            "rust/crates/dev-tools/src/journald_fixture.rs",
            "rust/crates/dev-tools/src/journald_native.rs",
            "rust/crates/dev-tools/src/bin/check_journald.rs",
            "rust/crates/dev-tools/src/bin/journalctl_fixture.rs",
        ])
    {
        let target = destination.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match fs::copy(root.join(path), target) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && RETIRED.contains(&path) => {}
            Err(e) => panic!("copy mandatory source {path}: {e}"),
        }
    }
}
fn run(root: &Path, output: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_check-journald"))
        .args([
            "--output",
            output.to_str().unwrap(),
            "--fixture",
            env!("CARGO_BIN_EXE_journalctl-fixture"),
        ])
        .env("OPENPILOT_RUST_ROOT", root)
        .output()
        .unwrap()
}
#[test]
fn every_original_pin_rejects_changed_source_before_child_creation() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/journald-check/policy-original.json")).unwrap();
    for path in corpus["sources"].as_object().unwrap().keys() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        copy_source_root(&root);
        fs::write(root.join(path), "source drift").unwrap();
        let output = temp.path().join("output");
        let child = run(&root, &output);
        assert!(!child.status.success(), "{path}");
        assert!(
            String::from_utf8_lossy(&child.stderr).contains("source drift"),
            "{path}: {:?}",
            child
        );
        assert!(!output.join("original").exists(), "{path}");
    }
}
#[test]
fn retained_log_reference_is_mandatory_and_reintroduced_tools_must_match() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    copy_source_root(&root);
    fs::remove_file(root.join("rust/tools/logmessaged_reference.py")).unwrap();
    let child = run(&root, &temp.path().join("missing-shared"));
    assert!(!child.status.success());
    fs::copy(
        source_root().join("rust/tools/logmessaged_reference.py"),
        root.join("rust/tools/logmessaged_reference.py"),
    )
    .unwrap();
    remove_retired(&root, "rust/tools/check_journald.py");
    fs::write(
        root.join("rust/tools/check_journald.py"),
        "reintroduced modified",
    )
    .unwrap();
    let child = run(&root, &temp.path().join("modified-reintroduction"));
    assert!(!child.status.success());
    assert!(String::from_utf8_lossy(&child.stderr).contains("source drift"));
}
#[test]
fn native_independent_reference_retains_all_original_process_cases_with_tools_absent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    copy_source_root(&root);
    for path in [
        "rust/tools/check_journald.py",
        "rust/tools/journald_peer.py",
        "rust/tools/journal_fixture_input.py",
        "rust/tools/journalctl_fixture.py",
        "rust/tools/tests/test_journal_fixture.py",
    ] {
        remove_retired(&root, path);
    }
    let output = temp.path().join("actual");
    let result = run(&root, &output);
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let actual: Value =
        serde_json::from_slice(&fs::read(output.join("report.json")).unwrap()).unwrap();
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/journald-check/process-original.json"
    ))
    .unwrap();
    let observed = &actual["original"];
    for name in [
        "messages",
        "errors",
        "disk_records",
        "exit",
        "child_survived",
    ] {
        assert_eq!(observed["normal"][name], original["normal"][name], "{name}");
    }
    for case in original["fatal"].as_array().unwrap() {
        let row = &observed[case["name"].as_str().unwrap()];
        assert_ne!(row["exit"], 0);
        assert_eq!(row["child_survived"], false);
        assert_eq!(
            row["trace"].as_array().unwrap().last().unwrap()["event"],
            case["last_event"]
        );
    }
    for (name, case) in original["lifecycle"].as_object().unwrap() {
        assert_eq!(observed["lifecycle"][name]["exit"], case["exit"], "{name}");
        assert_eq!(
            observed["lifecycle"][name]["child_survived"], case["child_survived"],
            "{name}"
        );
    }
    let inputs: Value =
        serde_json::from_slice(&fs::read(output.join("inputs.json")).unwrap()).unwrap();
    assert_eq!(
        inputs["checker_sha256"],
        format!(
            "{:x}",
            Sha256::digest(fs::read(env!("CARGO_BIN_EXE_check-journald")).unwrap())
        )
    );
}

#[test]
fn failed_actual_journal_child_is_detected_before_readiness_deadline() {
    let raw = include_bytes!("fixtures/journald-check/readiness-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "105ed8ab7893689f965efab06eae498510d32f8a2094527ce8bca9225764b6e4"
    );
    let original: Value = serde_json::from_slice(raw).unwrap();
    assert!(original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .all(
            |c| c["before_15_second_deadline"] == true && c["failure"]["type"] == "AssertionError"
        ));
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("failed.fixture");
    let result = Command::new(env!("CARGO_BIN_EXE_check-journald"))
        .arg("--output")
        .arg(&output)
        .args(["--fixture", "/bin/false"])
        .env("OPENPILOT_RUST_ROOT", source_root())
        .output()
        .unwrap();
    assert!(!result.status.success());
    let text = String::from_utf8_lossy(&result.stderr);
    assert!(
        text.contains("journal journal exited before readiness"),
        "{text}"
    );
    assert!(!text.contains("readiness timeout"), "{text}");
    let invocation = fs::read_to_string(output.join("original/normal/records.json")).unwrap();
    assert!(serde_json::from_str::<Value>(&invocation).is_ok());
}

#[test]
fn preexisting_case_directory_is_refused_before_child_or_artifact_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("actual");
    let case = output.join("original/normal");
    fs::create_dir_all(&case).unwrap();
    fs::write(case.join("marker"), "preserve").unwrap();
    fs::write(case.join("collector.stderr"), "prior collector").unwrap();
    let child = run(&source_root(), &output);
    assert!(!child.status.success());
    assert_eq!(fs::read(case.join("marker")).unwrap(), b"preserve");
    assert_eq!(
        fs::read(case.join("collector.stderr")).unwrap(),
        b"prior collector"
    );
    assert!(!case.join("child.jsonl").exists());
}
