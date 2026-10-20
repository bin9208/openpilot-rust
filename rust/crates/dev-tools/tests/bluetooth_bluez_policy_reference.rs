//! Mandatory live original/native report pair; no synthesized original fallback.
use std::{fs, path::PathBuf};

fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("required live BlueZ policy evidence missing: {name}"))
}

#[test]
fn original_python_312_and_native_controller_keep_full_corpus_and_receipt() {
    let original = required("BLUEZ_POLICY_ORIGINAL_OUTPUT");
    let native = required("BLUEZ_POLICY_NATIVE_OUTPUT");
    for name in [
        "cases.jsonl",
        "source.json",
        "native.jsonl",
        "native.stderr",
    ] {
        assert_eq!(
            fs::read(original.join(name)).unwrap(),
            fs::read(native.join(name)).unwrap(),
            "original/native file differs: {name}"
        );
    }
    let original_receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(original.join("result.json")).unwrap()).unwrap();
    let native_receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(native.join("result.json")).unwrap()).unwrap();
    assert_eq!(original_receipt, native_receipt);
    assert_eq!(original_receipt["cases"], 6363);
    assert_eq!(original_receipt["digit_points"], 808);
    assert_eq!(original_receipt["unicode_version"], "15.0.0");
    let stdout: serde_json::Value =
        serde_json::from_slice(&fs::read(required("BLUEZ_POLICY_ORIGINAL_STDOUT")).unwrap())
            .unwrap();
    assert_eq!(stdout, original_receipt);
    let native_stdout: serde_json::Value =
        serde_json::from_slice(&fs::read(required("BLUEZ_POLICY_NATIVE_STDOUT")).unwrap()).unwrap();
    assert_eq!(native_stdout, native_receipt);
    let _original_diagnostics = fs::read(required("BLUEZ_POLICY_ORIGINAL_STDERR")).unwrap();
    let _native_diagnostics = fs::read(required("BLUEZ_POLICY_NATIVE_STDERR")).unwrap();
    let digits: serde_json::Value =
        serde_json::from_slice(&fs::read(required("BLUEZ_POLICY_ORIGINAL_DIGITS")).unwrap())
            .unwrap();
    let native_digits: serde_json::Value =
        serde_json::from_slice(&fs::read(native.join("digits.stdout")).unwrap()).unwrap();
    assert_eq!(digits, native_digits);
    assert_eq!(digits.as_array().unwrap().len(), 808);
    assert_eq!(
        fs::read(required("BLUEZ_POLICY_ORIGINAL_DIGITS_STDERR")).unwrap(),
        fs::read(native.join("digits.stderr")).unwrap(),
        "original/native digit probe stderr differs"
    );
}
