//! Exercise the archive-independent CLI against an owned verified cache.
use sha2::{Digest, Sha256};
use std::{fs, process::Command};

#[test]
fn generic_asset_mode_reuses_authenticated_bytes_without_network_or_capture_paths() {
    let temp = tempfile::tempdir().unwrap();
    let bytes = b"owned asset CLI fixture, not a model";
    let sha = format!("{:x}", Sha256::digest(bytes));
    let cache = temp.path().join("cache");
    let object = cache.join("sha256").join(&sha);
    fs::create_dir_all(&object).unwrap();
    fs::write(object.join("blob"), bytes).unwrap();
    let plan = temp.path().join("assets.json");
    fs::write(&plan, serde_json::to_vec(&serde_json::json!({
        "schema": 1, "reserve_bytes": 0,
        "assets": {"owned": {"url": "https://owned.invalid/unused", "sha256": sha.clone(), "bytes": bytes.len()}}
    })).unwrap()).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-inputs"))
            .args(["--prepare-assets", "--plan"])
            .arg(&plan)
            .arg("--cache")
            .arg(&cache)
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(report["owned"]["sha256"], sha);
    assert_eq!(report["owned"]["transferred_bytes"], 0);
    assert_eq!(report["owned"]["stored_bytes"], 0);
    let mut corrupt = bytes.to_vec();
    corrupt[0] ^= 1;
    fs::write(object.join("blob"), &corrupt).unwrap();
    assert!(!run().status.success());
    assert_eq!(fs::read(object.join("blob")).unwrap(), corrupt);
}

#[test]
fn generic_asset_mode_rejects_capture_and_other_preparation_arguments_before_writing() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    for extra in [
        vec!["--root", "."],
        vec!["--output", "forbidden"],
        vec!["--store", "forbidden"],
        vec!["--prepare-python"],
        vec!["--prepare-assets"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-inputs"))
            .args(["--prepare-assets", "--cache"])
            .arg(&cache)
            .args(extra)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!cache.exists());
    }
}
