#![cfg(feature = "uploader-reference-native")]
use openpilot_dev_tools::uploader_reference_check::{load, run, Options};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
fn artifact() -> (PathBuf, String) {
    let path = PathBuf::from(
        std::env::var_os("UPLOADER_ORIGINAL_REFERENCE")
            .expect("mandatory original Actions capture path"),
    );
    let pin = std::env::var("UPLOADER_ORIGINAL_REFERENCE_SHA256")
        .expect("mandatory independently audited original capture SHA");
    (path, pin)
}
#[test]
fn real_original_capture_admits_and_wrong_digest_fails_closed() {
    let (path, pin) = artifact();
    load(&path, &pin).unwrap();
    assert!(load(&path, &"0".repeat(64)).is_err());
    let checker = std::env::var_os("UPLOADER_REFERENCE_CHECKER").expect("actual native CLI");
    let help = std::process::Command::new(&checker)
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout)
        .unwrap()
        .contains("--fixture-sha256"));
    let output = tempfile::tempdir().unwrap();
    let rejected = std::process::Command::new(checker)
        .arg("--binary")
        .arg(std::env::var_os("UPLOADER_TRACE_BINARY").unwrap())
        .arg("--fixture")
        .arg(path)
        .arg("--fixture-sha256")
        .arg("0".repeat(64))
        .arg("--output")
        .arg(output.path())
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(1));
    assert!(String::from_utf8(rejected.stderr)
        .unwrap()
        .contains("SHA mismatch"));
}
#[test]
fn validly_serialized_capture_with_unsafe_file_name_is_rejected() {
    let (path, _) = artifact();
    let mut data: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    data["cases"][0]["files"][0][0] = "../outside-root".into();
    let raw = serde_json::to_vec(&data).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let corrupt = temp.path().join("traversal.json");
    fs::write(&corrupt, &raw).unwrap();
    let pin = format!("{:x}", Sha256::digest(&raw));
    assert!(load(&corrupt, &pin).is_err());
}
#[test]
fn actual_native_controller_replays_complete_original_group() {
    let (fixture, fixture_sha256) = artifact();
    let output = PathBuf::from(
        std::env::var_os("UPLOADER_GROUP_PROOF").expect("mandatory retained proof directory"),
    )
    .join("ordinary");
    let result = run(&Options {
        root: PathBuf::from(
            std::env::var_os("OPENPILOT_RUST_ROOT").expect("mandatory source root"),
        ),
        binary: PathBuf::from(
            std::env::var_os("UPLOADER_TRACE_BINARY").expect("actual uploader_trace binary"),
        ),
        fixture,
        fixture_sha256,
        output,
    })
    .unwrap();
    assert_eq!(result["filesystem_scenarios"], 212);
    assert_eq!(result["backoff_decisions"], 10000);
    assert_eq!(result["loopback_http_scenarios"], 32);
}
