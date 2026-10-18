//! Real native Sentry HTTP transport, independent source policy, and SDK capture fields.
#![cfg(all(feature = "sdk-transport-native", feature = "tombstone-tools-native"))]
use openpilot_dev_tools::{sdk_transport_check, tombstone_reference_check::Options};
use openpilot_log_format::JsonValue as J;
use std::{fs, path::Path};
#[test]
fn fourteen_real_sdk_envelopes_and_native_io_exception_keep_original_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let required = |key| {
        fs::canonicalize(
            std::env::var_os(key)
                .unwrap_or_else(|| panic!("{key} must name verified current-source artifact")),
        )
        .unwrap()
    };
    let temp = tempfile::tempdir().unwrap();
    let options = Options {
        binary: required("TOMBSTONE_PROBE_BINARY"),
        params_binding: required("TOMBSTONE_PARAMS_BINDING"),
        runner: vec![],
        output: temp.path().join("sdk"),
        environment: std::env::vars_os().collect(),
    };
    let report = sdk_transport_check::run(&root, &options).unwrap();
    assert!(report.get("result").unwrap().text_eq("PASS"));
    assert_eq!(report.get("events").unwrap().to_json().unwrap(), "14");
    assert_eq!(
        report
            .get("native_io_exception_events")
            .unwrap()
            .to_json()
            .unwrap(),
        "1"
    );
    let disk =
        J::parse(&fs::read_to_string(options.output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(disk.to_json().unwrap(), report.to_json().unwrap());
    for name in (0..13).map(|index| format!("envelope-{index}.txt")).chain([
        "envelope-dirty.txt".into(),
        "envelope-native-error.txt".into(),
    ]) {
        assert!(!fs::read(options.output.join(name)).unwrap().is_empty());
    }
    let details =
        J::parse(&fs::read_to_string(options.output.join("native-error.json")).unwrap()).unwrap();
    assert!(details.get("event").is_some());
    assert!(options.output.join("policy/comparisons.jsonl").is_file());
}

#[test]
fn invalid_source_removes_prior_success_before_any_transport_work() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("sdk");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("manifest.json"), "{\"result\":\"PASS\"}\n").unwrap();
    let options = Options {
        binary: temp.path().join("missing-probe"),
        params_binding: temp.path().join("missing-binding"),
        runner: vec![],
        output,
        environment: std::env::vars_os().collect(),
    };
    assert!(sdk_transport_check::run(temp.path(), &options).is_err());
    assert!(!options.output.join("manifest.json").exists());
    assert!(!options.output.join("policy").exists());
}

#[test]
fn cli_failure_does_not_publish_or_retain_a_success_receipt() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let cli = env!("CARGO_BIN_EXE_check-crash-sdk-transport");
    for args in [vec![], vec!["--source"], vec!["missing", "binding"]] {
        let result = Command::new(cli)
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    let output = temp.path().join("result");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("manifest.json"), "{\"result\":\"PASS\"}\n").unwrap();
    let result = Command::new(cli)
        .args(["missing", "binding", "result"])
        .env("OPENPILOT_RUST_ROOT", temp.path())
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert!(!output.join("manifest.json").exists());
    assert!(!output.join("policy").exists());
}
