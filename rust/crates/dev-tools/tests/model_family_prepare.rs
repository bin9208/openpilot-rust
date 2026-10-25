//! Hosted-only preparation acceptance. Downloads are independently SHA-pinned
//! data inputs; this test stages once and leaves that exact bundle for reuse.
#![cfg(all(target_os = "linux", feature = "driver-daemon-native"))]
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use std::{ffi::OsString, fs, path::PathBuf, time::Duration};
fn required(name: &str) -> OsString {
    std::env::var_os(name).unwrap_or_else(|| panic!("missing {name}"))
}
#[test]
#[ignore = "hosted authenticated USB archive preparation; no model execution"]
fn actual_usb_stage_preserves_full_45_asset_and_three_license_rosters() {
    let output = PathBuf::from(required("MODEL_FAMILY_USB_STAGE_OUTPUT"));
    let mut spec = CommandSpec::new_os(
        vec![
            env!("CARGO_BIN_EXE_stage-model-family-usb-assets").into(),
            "--release-archive".into(),
            required("MODEL_FAMILY_USB_RELEASE_ARCHIVE"),
            "--runtime-archive".into(),
            required("MODEL_FAMILY_USB_RUNTIME_ARCHIVE"),
            "--installed".into(),
            required("MODEL_FAMILY_USB_INSTALLED"),
            "--output".into(),
            output.as_os_str().to_owned(),
        ],
        Duration::from_secs(1200),
    );
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["assets_files"].as_object().unwrap().len(), 45);
    assert_eq!(report["licenses"].as_object().unwrap().len(), 3);
    assert_eq!(report["model_pickle_executed"], false);
    assert_eq!(report["release_executables_extracted"], false);
    assert_eq!(
        report,
        serde_json::from_slice::<Value>(
            &fs::read(output.join("usb-asset-admission.json")).unwrap()
        )
        .unwrap()
    );
    assert_eq!(fs::metadata(output.join("usb-assets/models/e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b/model.pkl")).unwrap().len(),776634338);
    // A second invocation must not overwrite this already-admitted bundle.
    fs::write(output.join("foreign-control"), b"preserve").unwrap();
    let rejected = NativeRunner.run(&spec).unwrap();
    assert_ne!(rejected.code, 0);
    assert_eq!(
        fs::read(output.join("foreign-control")).unwrap(),
        b"preserve"
    );
    assert!(rejected.stdout.is_empty());
}
