use openpilot_runtime_package::{Artifact, Manifest, VerifiedManifest};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
fn fixture() -> (tempfile::TempDir, String) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("rust/bin/signal/detector")).unwrap();
    fs::write(
        root.path().join("rust/bin/signal/detector/graph.json"),
        b"real fixture graph",
    )
    .unwrap();
    let manifest = Manifest {
        schema: 1,
        source_commit: "e8e152dbcfd9574a1791ed12026d1880e87689b3".into(),
        agnos_version: "19.8-carrot-bt1".into(),
        files: vec![Artifact {
            path: "rust/bin/signal/detector/graph.json".into(),
            sha256: format!("{:x}", Sha256::digest(b"real fixture graph")),
            executable: false,
        }],
    };
    fs::write(
        root.path().join("build.json"),
        serde_json::to_vec(&serde_json::json!({"openpilot":{"git_commit":manifest.source_commit}}))
            .unwrap(),
    )
    .unwrap();
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let digest = format!("{:x}", Sha256::digest(&bytes));
    fs::write(root.path().join("rust/runtime-manifest.json"), bytes).unwrap();
    (root, digest)
}
#[test]
fn verified_descendant_identity_reuses_policy_and_rejects_tampering() {
    let (root, digest) = fixture();
    let admitted = VerifiedManifest::read_expected(root.path(), &digest).unwrap();
    assert_eq!(admitted.digest(), digest);
    assert_eq!(
        admitted.source_commit(),
        "e8e152dbcfd9574a1791ed12026d1880e87689b3"
    );
    let (path, hash) = admitted
        .artifact(Path::new("rust/bin/signal/detector/graph.json"))
        .unwrap();
    assert_eq!(fs::read(path).unwrap(), b"real fixture graph");
    assert_eq!(
        admitted
            .read_artifact(Path::new("rust/bin/signal/detector/graph.json"), 128)
            .unwrap(),
        b"real fixture graph"
    );
    assert!(admitted
        .read_artifact(Path::new("rust/bin/signal/detector/graph.json"), 2)
        .is_err());
    assert_eq!(hash, format!("{:x}", Sha256::digest(b"real fixture graph")));
    assert!(admitted.artifact(Path::new("../outside")).is_err());
    assert!(admitted.artifact(Path::new("missing")).is_err());
    assert!(VerifiedManifest::read_expected(root.path(), &"0".repeat(64)).is_err());
    fs::write(root.path().join("rust/bin/unlisted"), b"unlisted").unwrap();
    assert!(VerifiedManifest::read_expected(root.path(), &digest).is_err());
    fs::remove_file(root.path().join("rust/bin/unlisted")).unwrap();
    fs::write(
        root.path().join("rust/bin/signal/detector/graph.json"),
        b"changed",
    )
    .unwrap();
    assert!(VerifiedManifest::read_expected(root.path(), &digest).is_err());
    assert!(admitted
        .read_artifact(Path::new("rust/bin/signal/detector/graph.json"), 128)
        .is_err());
}
#[test]
fn identity_mismatch_or_changed_manifest_never_admits_artifacts() {
    let (root, digest) = fixture();
    fs::write(
        root.path().join("build.json"),
        b"{\"openpilot\":{\"git_commit\":\"wrong\"}}",
    )
    .unwrap();
    assert!(VerifiedManifest::read_expected(root.path(), &digest).is_err());
    fs::write(root.path().join("rust/runtime-manifest.json"), b"{}").unwrap();
    assert!(VerifiedManifest::read_expected(root.path(), &digest).is_err());
    assert!(VerifiedManifest::read_expected(root.path(), "untrusted").is_err());
}

#[test]
fn manager_command_replaces_stale_identity_without_mutating_parent_environment() {
    let parent = std::env::var_os("CARROT_RUNTIME_MANIFEST_SHA256");
    let mut command = std::process::Command::new("unused-owned-command");
    command
        .env("CARROT_RUNTIME_MANIFEST_SHA256", "stale")
        .env("CARROT_RUNTIME_ROOT", "old");
    let digest = "a".repeat(64);
    Manifest::bind_descendant(&mut command, Path::new("/owned/runtime"), &digest).unwrap();
    let values: std::collections::BTreeMap<_, _> = command
        .get_envs()
        .map(|(key, value)| (key.to_os_string(), value.map(ToOwned::to_owned)))
        .collect();
    assert_eq!(
        values[std::ffi::OsStr::new("CARROT_RUNTIME_MANIFEST_SHA256")],
        Some(digest.into())
    );
    assert_eq!(
        values[std::ffi::OsStr::new("CARROT_RUNTIME_ROOT")],
        Some("/owned/runtime".into())
    );
    assert_eq!(std::env::var_os("CARROT_RUNTIME_MANIFEST_SHA256"), parent);
    assert!(
        Manifest::bind_descendant(&mut command, Path::new("/owned/runtime"), "invalid").is_err()
    );
}
