//! Current candidates, independent source policy, both actual collectors.
#![cfg(feature = "managed-entry-native")]
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

fn artifact(name: &str) -> PathBuf {
    let path =
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("mandatory {name} missing")))
            .canonicalize()
            .unwrap();
    assert!(fs::read(&path).unwrap().starts_with(b"\x7fELF"));
    path
}
#[test]
fn invalid_selection_cannot_publish_zero_comparison_success() {
    let output = tempfile::tempdir().unwrap();
    fs::write(
        output.path().join("manifest.json"),
        r#"{"result":"PASS","comparisons":44}"#,
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_check-managed-entry"))
        .args(["/bin/true", "/bin/true", "/missing-binding"])
        .arg(output.path())
        .args(["--case", "nonexistent-control"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.path().join("manifest.json").exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unknown managed-entry case"));
}
#[test]
fn all44_actual_collector_source_native_comparisons_and_no_stale_success() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let binary = artifact("MANAGED_CHILD_BINARY");
    let collector = artifact("MANAGED_COLLECTOR_BINARY");
    let binding = PathBuf::from(
        std::env::var_os("MANAGED_PARAMS_BINDING").expect("mandatory original binding provenance"),
    );
    let output = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_check-managed-entry"));
    command
        .args([&binary, &collector, &binding])
        .arg(output.path())
        .env("OPENPILOT_RUST_ROOT", &root)
        .env("PATH", "/nonexistent");
    let result = command.output().unwrap();
    if !result.status.success() {
        let retained = output.keep();
        panic!(
            "managed actual CLI failed; evidence {}: {}",
            retained.display(),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let report: Value =
        serde_json::from_slice(&fs::read(output.path().join("manifest.json")).unwrap()).unwrap();
    assert_eq!(report["result"], "PASS");
    assert_eq!(report["comparisons"], 44);
    assert_eq!(report["results"].as_array().unwrap().len(), 44);
    for collector in ["original", "native"] {
        for (name, _) in openpilot_dev_tools::managed_entry_check::cases() {
            for kind in ["source", "native"] {
                let case = output
                    .path()
                    .join(format!("{collector}-collector/{name}/{kind}/result.json"));
                let value: Value = serde_json::from_slice(&fs::read(&case).unwrap()).unwrap();
                assert_eq!(value["result"], "PASS", "{}", case.display());
                assert!(value["ready"]["ready"].as_bool().unwrap());
            }
        }
    }
    // Existing destinations cannot silently reuse the earlier evidence. Failure
    // on the first already-present case must remove the old aggregate PASS.
    let repeat = command.output().unwrap();
    assert!(!repeat.status.success());
    assert!(!output.path().join("manifest.json").exists());
}
