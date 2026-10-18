//! Original argument failures must precede creation of the output directory.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::Path, process::Command};
#[test]
fn original_four_checker_cli_help_and_required_argument_boundaries() {
    let bytes = include_bytes!("fixtures/timed-tools/cli-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "e003536ede4cb062759c9941dccc61d22342a190c50c3cb582c7a8bcd861f077"
    );
    let original: Value = serde_json::from_slice(bytes).unwrap();
    let mut count = 0;
    for row in original["cases"].as_array().unwrap() {
        let binary = match row["script"].as_str().unwrap() {
            "check_timed_reference.py" => env!("CARGO_BIN_EXE_check-timed-reference"),
            "check_timed_dates.py" => env!("CARGO_BIN_EXE_check-timed-dates"),
            "check_timed_http.py" => env!("CARGO_BIN_EXE_check-timed-http"),
            "check_timed_logging.py" => env!("CARGO_BIN_EXE_check-timed-logging"),
            _ => continue,
        };
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output");
        let args = row["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| {
                arg.as_str()
                    .unwrap()
                    .replace("<output>", output.to_str().unwrap())
            })
            .collect::<Vec<_>>();
        let captured = Command::new(binary)
            .args(&args)
            .env(
                "OPENPILOT_RUST_ROOT",
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../..")),
            )
            .output()
            .unwrap();
        assert_eq!(
            captured.status.code(),
            row["exit_code"].as_i64().map(|code| code as i32),
            "{row}"
        );
        assert_eq!(
            String::from_utf8(captured.stdout).unwrap(),
            row["stdout"].as_str().unwrap(),
            "{row}"
        );
        assert_eq!(
            String::from_utf8(captured.stderr).unwrap(),
            row["stderr"].as_str().unwrap(),
            "{row}"
        );
        assert_eq!(
            output.exists(),
            row["output_exists"].as_bool().unwrap(),
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 20);
}
