//! Actual original argparse admissions are captured separately before deletion.
#![cfg(target_os = "linux")]
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{env, ffi::OsString, fs, os::unix::ffi::OsStringExt, path::PathBuf, time::Duration};

const ORIGINAL_SHA: &str = "82be3830f8e468c32abe583c197cdf2777cd5fb748a795a8c70247a7dcd00059";
const NAMES: &[&str] = &[
    "short-help",
    "long-help",
    "repeated-short-help",
    "attached-short-help",
    "repeated-binary-before-help",
    "abbreviated-options-before-help",
    "missing-binary-value",
    "unknown-option",
    "incomplete-required-set",
    "help-equals",
    "non-utf8-option-value-before-help",
];

fn decode_hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap())
        .collect()
}

#[test]
fn native_cli_preserves_original_help_error_and_raw_argv_admissions() {
    let path = PathBuf::from(
        env::var_os("UPDATED_DAEMON_CLI_ORIGINAL_RECEIPT")
            .expect("fresh original CLI capture or pinned original-only fixture required"),
    );
    let bytes = fs::read(&path).unwrap();
    let expected_sha =
        env::var("UPDATED_DAEMON_CLI_ORIGINAL_SHA256").expect("original CLI receipt SHA required");
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected_sha);
    let receipt: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(receipt["source_sha256"], ORIGINAL_SHA);
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), NAMES.len());
    let native = PathBuf::from(env!("CARGO_BIN_EXE_check-updated-daemon"));
    for (index, observation) in cases.iter().enumerate() {
        assert_eq!(observation["name"], NAMES[index]);
        let args = observation["argv_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hex| OsString::from_vec(decode_hex(hex.as_str().unwrap())))
            .collect::<Vec<_>>();
        let mut command = vec![native.as_os_str().to_owned()];
        command.extend(args);
        let result = NativeRunner
            .run(&CommandSpec::new_os(command, Duration::from_secs(5)))
            .unwrap();
        let original_code = observation["code"].as_i64().unwrap() as i32;
        let original_stdout = decode_hex(observation["stdout_hex"].as_str().unwrap());
        let original_stderr = decode_hex(observation["stderr_hex"].as_str().unwrap());
        assert!(matches!(original_code, 0 | 2));
        assert_eq!(result.code, original_code, "{} status", NAMES[index]);
        if original_code == 0 {
            assert!(original_stderr.is_empty());
            assert!(!original_stdout.is_empty());
            assert!(result.stderr.is_empty());
            assert!(String::from_utf8_lossy(&result.stdout)
                .to_ascii_lowercase()
                .contains("usage:"));
        } else {
            assert!(original_stdout.is_empty());
            assert!(!original_stderr.is_empty());
            assert!(result.stdout.is_empty());
            assert!(!result.stderr.is_empty());
        }
    }
}
