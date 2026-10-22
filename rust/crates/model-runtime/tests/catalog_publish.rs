#![cfg(all(feature = "native-skip-miri", target_os = "linux"))]

#[path = "../src/catalog_publish.rs"]
mod catalog_publish;

use catalog_publish::publish;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};

fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn stage(root: &Path, suffix: &str) -> (PathBuf, PathBuf) {
    let parent = root.join(format!(".build-{suffix}"));
    let generation = parent.join("generation");
    let bundle = generation.join("driver-1344x760");
    fs::create_dir_all(&bundle).unwrap();
    // Publication-only data; these deliberately are not executable model assets.
    let graph = br#"{"publication-test-only":true}"#;
    let pipeline = br#"{"publication-test-only":true}"#;
    fs::write(bundle.join("graph.json"), graph).unwrap();
    fs::write(bundle.join("pipeline.json"), pipeline).unwrap();
    fs::write(bundle.join("weights.bin"), b"emitter-owned inert bytes").unwrap();
    let request = parent.join("entries.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!([{
            "camera":[1344,760], "directory":"driver-1344x760", "kind":"driver",
            "graph_sha256":digest(graph), "pipeline_sha256":digest(pipeline)
        }]))
        .unwrap(),
    )
    .unwrap();
    (generation, request)
}
#[test]
fn reuses_identical_generation_without_mutating_bundle_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (first, request) = stage(root, "first");
    let initial = publish(root, &first, &request).unwrap();
    assert!(!initial.reused && !first.exists());
    let generation = root.join(&initial.generation);
    assert_eq!(
        digest(&fs::read(generation.join("index.json")).unwrap()),
        initial.generation
    );
    let pointer = fs::read(root.join("current.json")).unwrap();
    let weights = fs::read(generation.join("driver-1344x760/weights.bin")).unwrap();
    let (second, request) = stage(root, "second");
    let repeated = publish(root, &second, &request).unwrap();
    assert!(repeated.reused && second.exists());
    assert_eq!(repeated.generation, initial.generation);
    assert_eq!(fs::read(root.join("current.json")).unwrap(), pointer);
    assert_eq!(
        fs::read(generation.join("driver-1344x760/weights.bin")).unwrap(),
        weights
    );
}
#[test]
fn corrupted_reused_generation_preserves_pointer_and_foreign_bytes() {
    let root = tempfile::tempdir().unwrap();
    let (first, request) = stage(root.path(), "first");
    let initial = publish(root.path(), &first, &request).unwrap();
    let pointer = fs::read(root.path().join("current.json")).unwrap();
    let weights = root
        .path()
        .join(initial.generation)
        .join("driver-1344x760/weights.bin");
    fs::write(&weights, b"independent replacement").unwrap();
    let (second, request) = stage(root.path(), "second");
    assert!(publish(root.path(), &second, &request).is_err());
    assert_eq!(fs::read(weights).unwrap(), b"independent replacement");
    assert_eq!(fs::read(root.path().join("current.json")).unwrap(), pointer);
    assert!(!second.join("index.json").exists());
}
#[test]
fn directory_roster_difference_cannot_reuse_an_existing_generation() {
    let root = tempfile::tempdir().unwrap();
    let (first, request) = stage(root.path(), "first");
    let initial = publish(root.path(), &first, &request).unwrap();
    fs::create_dir(
        root.path()
            .join(initial.generation)
            .join("driver-1344x760/extra-empty"),
    )
    .unwrap();
    let pointer = fs::read(root.path().join("current.json")).unwrap();
    let (second, request) = stage(root.path(), "second");
    assert!(publish(root.path(), &second, &request).is_err());
    assert_eq!(fs::read(root.path().join("current.json")).unwrap(), pointer);
}
#[test]
fn staged_alias_and_preexisting_index_are_never_followed_or_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let (generation, request) = stage(root.path(), "alias");
    let outside = root.path().join("foreign");
    fs::write(&outside, b"foreign bytes").unwrap();
    symlink(&outside, generation.join("driver-1344x760/alias")).unwrap();
    assert!(publish(root.path(), &generation, &request).is_err());
    assert!(
        fs::symlink_metadata(generation.join("driver-1344x760/alias"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&outside).unwrap(), b"foreign bytes");
    fs::remove_file(generation.join("driver-1344x760/alias")).unwrap();
    fs::write(generation.join("index.json"), b"caller-owned collision").unwrap();
    assert!(publish(root.path(), &generation, &request).is_err());
    assert_eq!(
        fs::read(generation.join("index.json")).unwrap(),
        b"caller-owned collision"
    );
    assert!(!root.path().join("current.json").exists());
}
#[test]
fn malformed_entries_fail_before_creating_any_index_or_pointer() {
    for bad in [
        json!([]),
        json!([{"kind":"driver"}]),
        json!([{
            "camera":[1344,760], "directory":"../escape", "kind":"driver",
            "graph_sha256":"00".repeat(32), "pipeline_sha256":"00".repeat(32)
        }]),
    ] {
        let root = tempfile::tempdir().unwrap();
        let (generation, request) = stage(root.path(), "bad");
        fs::write(&request, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(publish(root.path(), &generation, &request).is_err());
        assert!(!generation.join("index.json").exists());
        assert!(!root.path().join("current.json").exists());
    }
}
#[test]
fn replacing_current_symlink_leaves_its_target_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("independent-pointer");
    fs::write(&outside, b"unrelated pointer content").unwrap();
    symlink(&outside, root.path().join("current.json")).unwrap();
    let (generation, request) = stage(root.path(), "valid");
    publish(root.path(), &generation, &request).unwrap();
    assert_eq!(fs::read(outside).unwrap(), b"unrelated pointer content");
    assert!(fs::symlink_metadata(root.path().join("current.json"))
        .unwrap()
        .is_file());
}
#[test]
fn pointer_failure_retains_new_immutable_generation_and_foreign_directory() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("current.json")).unwrap();
    fs::write(root.path().join("current.json/foreign"), b"keep").unwrap();
    let (generation, request) = stage(root.path(), "failure");
    assert!(publish(root.path(), &generation, &request).is_err());
    assert_eq!(
        fs::read(root.path().join("current.json/foreign")).unwrap(),
        b"keep"
    );
    assert!(!generation.exists());
    let hashes = fs::read_dir(root.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().len() == 64)
        .count();
    assert_eq!(hashes, 1);
}
#[test]
fn cli_has_no_python_or_compiler_command_requirement() {
    let root = tempfile::tempdir().unwrap();
    let (generation, request) = stage(root.path(), "cli");
    let result = Command::new(env!("CARGO_BIN_EXE_publish-model-catalog"))
        .env("PATH", "/nonexistent")
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "--staged-generation",
            generation.to_str().unwrap(),
            "--entries",
            request.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(receipt["reused"], false);
    assert_eq!(receipt["generation"].as_str().unwrap().len(), 64);
}
