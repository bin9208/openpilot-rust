use openpilot_dev_tools::hardware_command_check;
use serde_json::Value;
use std::fs;
fn decode(source: &str) -> Vec<u8> {
    source
        .as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn all_thirty_original_json_equality_and_exact_stderr_cases_match() {
    let original: Value =
        serde_json::from_str(include_str!("fixtures/hardware-command/original.json")).unwrap();
    for case in original["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("command.json"),
            decode(case["json_hex"].as_str().unwrap()),
        )
        .unwrap();
        fs::write(
            temp.path().join("command.stderr"),
            decode(case["stderr_hex"].as_str().unwrap()),
        )
        .unwrap();
        let result = hardware_command_check::check(temp.path());
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{}: {result:?}",
            case["name"]
        );
    }
}
#[test]
fn missing_artifacts_and_invalid_utf8_cannot_pass() {
    let temp = tempfile::tempdir().unwrap();
    assert!(hardware_command_check::check(temp.path()).is_err());
    fs::write(temp.path().join("command.json"), b"\xff").unwrap();
    assert!(hardware_command_check::check(temp.path()).is_err());
}
#[test]
fn actual_cli_returns_nonzero_for_rejection_and_prints_no_success_artifact() {
    use std::process::Command;
    let original: Value =
        serde_json::from_str(include_str!("fixtures/hardware-command/original.json")).unwrap();
    for case in [
        &original["cases"][0],
        &original["cases"][6],
        &original["cases"][25],
    ] {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("command.json"),
            decode(case["json_hex"].as_str().unwrap()),
        )
        .unwrap();
        fs::write(
            temp.path().join("command.stderr"),
            decode(case["stderr_hex"].as_str().unwrap()),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_check-hardware-command"))
            .arg("--evidence")
            .arg(temp.path())
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            case["error"].is_null(),
            "{}",
            case["name"]
        );
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr.is_empty(), case["error"].is_null());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    }
    for args in [vec![], vec!["--evidence"], vec!["--unknown", "/tmp"]] {
        assert!(!Command::new(env!("CARGO_BIN_EXE_check-hardware-command"))
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    }
}
