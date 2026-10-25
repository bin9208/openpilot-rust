#![cfg(feature = "paramsd-startup-check-native")]
use openpilot_dev_tools::paramsd_startup_fixture::cases;
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn seven_startup_cases_match_original_order_and_seed_kinds() {
    let cases = cases();
    assert_eq!(
        cases.iter().map(|case| case.name).collect::<Vec<_>>(),
        [
            "blocked-car",
            "malformed-car",
            "migration",
            "invalid-json",
            "wrong-fields",
            "malformed-cache",
            "wrong-covariance-dimensions",
        ]
    );
    assert!(cases[0].seed.is_empty());
    assert_eq!(cases[1].seed["CarParams"], [1, 2]);
    assert_eq!(cases[5].seed["LiveParametersV2"], [1, 2]);
}

#[test]
fn cli_rejects_incomplete_unknown_duplicate_and_non_native_binary() {
    let binary = env!("CARGO_BIN_EXE_check-paramsd-startup-native");
    for (arguments, expected) in [
        (vec![], "--root required"),
        (vec!["--root", "/", "--root", "/"], "duplicate option"),
        (vec!["--unknown", "value"], "unknown option"),
        (vec!["--root"], "option value missing"),
    ] {
        let result = Command::new(binary).args(arguments).output().unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains(expected));
    }
    let temporary = tempfile::tempdir().unwrap();
    let script = temporary.path().join("script");
    fs::write(&script, b"#!/bin/sh\nexit 0\n").unwrap();
    let output = temporary.path().join("output");
    let result = Command::new(binary)
        .args(["--root", "/", "--binary"])
        .arg(script)
        .arg("--evidence")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
}

// A fresh source checker is mandatory; never substitute a native-generated
// expected request or historical original data for this pre-retirement gate.
#[test]
#[ignore = "hosted fresh original/native gate only"]
fn all_seed_bytes_match_fresh_original_controller_calls() {
    let path = PathBuf::from(
        std::env::var_os("PARAMSD_STARTUP_ORIGINAL_INPUT").expect("fresh original input required"),
    );
    let original: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(cases()).unwrap(), original);
}
