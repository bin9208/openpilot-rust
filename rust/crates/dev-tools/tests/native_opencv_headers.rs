use std::{path::Path, process::Command};

#[test]
fn released_headers_reject_legacy_build_arguments_without_creating_paths() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let store = temp.path().join("store");
    let result = Command::new(env!("CARGO_BIN_EXE_build-runtime-opencv"))
        .args(["--prepare-released-headers", "--source", "must-not-build"])
        .arg("--cache")
        .arg(&cache)
        .arg("--store")
        .arg(&store)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("invalid, mixed or duplicate"));
    assert!(!cache.exists() && !store.exists());
}

#[test]
fn changed_generated_header_is_rejected_before_downloading() {
    let temp = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native-opencv-headers.json");
    let mut plan: serde_json::Value =
        serde_json::from_slice(&std::fs::read(source).unwrap()).unwrap();
    plan["generated"]["opencv2/opencv_modules.hpp"] = "changed".into();
    let path = temp.path().join("plan.json");
    std::fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
    let cache = temp.path().join("cache");
    let store = temp.path().join("store");
    let result = Command::new(env!("CARGO_BIN_EXE_build-runtime-opencv"))
        .arg("--prepare-released-headers")
        .arg("--plan")
        .arg(path)
        .arg("--cache")
        .arg(&cache)
        .arg("--store")
        .arg(&store)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("released provider bytes"));
    assert!(!cache.exists() && !store.exists());
}
