//! Independent existing-checker observations, retained before native implementation.
use openpilot_dev_tools::qcom_assistance_check::{
    configuration, parse, run, validate_artifacts, validate_source,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, path::Path};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/qcom-assistance-tools")
            .join(name),
    )
    .unwrap()
}

#[test]
fn original_success_failure_and_cli_captures_are_immutable() {
    for (name, sha) in [
        (
            "results-original.json",
            "c6fda9eaa4e198a6b75baad638288b9a7bbb2505f113140773c6b3aa2edf8367",
        ),
        (
            "failure-original.json",
            "29c94ee245a77208e8a7504f2cc68d5c56f84e12878746a1ea2031c7f42e81a0",
        ),
        (
            "cli-original.json",
            "3c53af270c70972bea435a533d3e1a77d290246fe66fb7c90190f7ad15e99a8b",
        ),
        (
            "config-original.json",
            "1677604af578384744db1e544573ab326a7657bc2abda874e646913a4a6273bf",
        ),
    ] {
        assert_eq!(
            format!("{:x}", Sha256::digest(fixture(name))),
            sha,
            "{name}"
        );
    }
}

#[test]
fn worker_configuration_preserves_all_original_private_path_and_mode_fields() {
    let cases: Value = serde_json::from_slice(&fixture("config-original.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let mode = case["mode"].as_str().unwrap();
        let actual = configuration(Path::new("/private-test-root"), 12345, mode).unwrap();
        let normalized = actual
            .to_json()
            .unwrap()
            .replace("/private-test-root", "$ROOT")
            .replace("127.0.0.1:12345", "127.0.0.1:$PORT");
        assert_eq!(
            serde_json::from_str::<Value>(&normalized).unwrap(),
            case["config"],
            "{mode}"
        );
    }
}

#[test]
fn original_cli_admission_and_help_codes_are_preserved() {
    let cases: Value = serde_json::from_slice(&fixture("cli-original.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let args = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| OsString::from(v.as_str().unwrap()))
            .collect::<Vec<_>>();
        let result = parse(&args);
        let code = if result.is_err() { 2 } else { 0 };
        assert_eq!(code, case["code"].as_i64().unwrap(), "{args:?}");
    }
    let options = parse(&["candidate".into(), "evidence".into()])
        .unwrap()
        .unwrap();
    assert_eq!(options.binary, Path::new("candidate"));
    assert_eq!(options.output, Path::new("evidence"));
}

#[test]
fn original_early_failures_keep_logs_and_never_create_success_or_later_scenarios() {
    let cases: Value = serde_json::from_slice(&fixture("failure-original.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        let missing = temp.path().join("missing-child");
        let binary = match case["name"].as_str().unwrap() {
            "missing-child" => missing.as_path(),
            "early-zero-child" => Path::new("/bin/true"),
            "output-file" => {
                fs::write(&output, b"preserve\n").unwrap();
                Path::new("/bin/false")
            }
            _ => Path::new("/bin/false"),
        };
        assert!(run(binary, &output).is_err(), "{}", case["name"]);
        assert_eq!(
            output.join("alternate/worker.log").is_file(),
            case["worker_log"].as_bool().unwrap()
        );
        assert!(!output.join("alternate/result.json").exists());
        assert!(!output.join("http404").exists());
        if case["name"] == "output-file" {
            assert_eq!(fs::read(output).unwrap(), b"preserve\n");
        }
    }
}

#[test]
fn actual_native_assistance_worker_preserves_all_four_original_scenarios() {
    let binary = std::env::var_os("QCOMGPSD_BINARY")
        .expect("QCOMGPSD_BINARY must identify the current source-matched daemon");
    let expected: Value = serde_json::from_slice(&fixture("results-original.json")).unwrap();
    let output = tempfile::tempdir().unwrap();
    run(Path::new(&binary), output.path()).unwrap();
    for mode in ["alternate", "http404", "oversize", "retry"] {
        let result: Value = serde_json::from_slice(
            &fs::read(output.path().join(mode).join("result.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(result, expected[mode], "{mode}");
    }
}

#[test]
fn original_artifact_assertions_reject_equal_length_corruption_and_missing_downloads() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("assist");
    let alternate = b"alternate replaces stale";
    fs::write(root.path().join("alternate"), alternate).unwrap();
    fs::write(&path, alternate).unwrap();
    assert!(validate_artifacts(root.path(), "alternate", &[], 0).is_ok());
    fs::write(&path, vec![b'?'; alternate.len()]).unwrap();
    assert!(validate_artifacts(root.path(), "alternate", &[], 0).is_err());
    fs::write(&path, alternate).unwrap();
    assert!(validate_artifacts(root.path(), "alternate", &["/assist".into()], 0).is_err());

    for (mode, count) in [("http404", 1), ("retry", 2)] {
        let attempts = vec!["/assist".into(); count];
        fs::write(&path, b"synthetic assistance").unwrap();
        assert!(validate_artifacts(root.path(), mode, &attempts, 0).is_ok());
        assert!(validate_artifacts(root.path(), mode, &attempts, 1).is_err());
        assert!(validate_artifacts(root.path(), mode, &[], 0).is_err());
        fs::write(&path, vec![b'?'; 20]).unwrap();
        assert!(validate_artifacts(root.path(), mode, &attempts, 0).is_err());
    }

    fs::remove_file(&path).unwrap();
    let attempts = vec!["/assist".into()];
    assert!(validate_artifacts(root.path(), "oversize", &attempts, 0).is_err());
    fs::write(root.path().join("assist.download"), vec![b'x'; 100_000]).unwrap();
    assert!(validate_artifacts(root.path(), "oversize", &attempts, 0).is_err());
    fs::write(root.path().join("assist.download"), vec![b'x'; 100_001]).unwrap();
    assert!(validate_artifacts(root.path(), "oversize", &attempts, 0).is_ok());
    fs::write(&path, b"stale").unwrap();
    assert!(validate_artifacts(root.path(), "oversize", &attempts, 0).is_err());
}

#[test]
fn retired_original_may_be_absent_but_a_changed_present_copy_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("rust/tools/check_qcomgps_assistance.py");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    assert!(validate_source(root.path()).is_ok());
    fs::write(&path, b"# changed or reintroduced original\n").unwrap();
    assert!(validate_source(root.path()).is_err());
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(validate_source(root.path()).is_err());
}
