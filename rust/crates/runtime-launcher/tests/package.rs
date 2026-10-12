use openpilot_runtime_launcher::package::{Artifact, Manifest};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt};

fn fixture() -> (tempfile::TempDir, Manifest) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("bin")).unwrap();
    fs::write(root.path().join("bin/service"), b"owned executable bytes").unwrap();
    fs::set_permissions(
        root.path().join("bin/service"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let manifest = Manifest {
        schema: 1,
        source_commit: "0123456789abcdef0123456789abcdef01234567".into(),
        agnos_version: "19.8-carrot-bt1".into(),
        files: vec![Artifact {
            path: "bin/service".into(),
            sha256: format!("{:x}", Sha256::digest(b"owned executable bytes")),
            executable: true,
        }],
    };
    (root, manifest)
}

#[test]
fn changed_missing_or_unlisted_executable_prevents_startup() {
    let (root, manifest) = fixture();
    manifest.verify(root.path(), &["bin/service"]).unwrap();
    assert!(manifest.verify(root.path(), &["bin/absent"]).is_err());
    fs::write(root.path().join("bin/service"), b"corrupt").unwrap();
    assert!(manifest.verify(root.path(), &["bin/service"]).is_err());
    fs::remove_file(root.path().join("bin/service")).unwrap();
    assert!(manifest.verify(root.path(), &["bin/service"]).is_err());
}

#[test]
fn executable_mode_duplicate_paths_and_parent_escape_are_rejected() {
    let (root, mut manifest) = fixture();
    fs::set_permissions(
        root.path().join("bin/service"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(manifest.verify(root.path(), &["bin/service"]).is_err());
    fs::set_permissions(
        root.path().join("bin/service"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    manifest.files.push(manifest.files[0].clone());
    assert!(manifest.verify(root.path(), &["bin/service"]).is_err());
    manifest.files.pop();
    manifest.files[0].path = "../outside".into();
    assert!(manifest.verify(root.path(), &[]).is_err());
}

#[test]
fn immutable_bundle_may_link_to_its_own_libraries_but_not_outside() {
    let (root, mut manifest) = fixture();
    std::os::unix::fs::symlink("service", root.path().join("bin/alias")).unwrap();
    manifest.files[0].path = "bin/alias".into();
    manifest.verify(root.path(), &["bin/alias"]).unwrap();
    fs::remove_file(root.path().join("bin/alias")).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", root.path().join("bin/alias")).unwrap();
    assert!(manifest.verify(root.path(), &[]).is_err());
}

#[test]
fn package_identity_must_match_runtime_build_metadata() {
    let (root, manifest) = fixture();
    fs::write(
        root.path().join("build.json"),
        serde_json::to_vec(
            &serde_json::json!({"openpilot": {"git_commit": manifest.source_commit}}),
        )
        .unwrap(),
    )
    .unwrap();
    manifest.verify_identity(root.path()).unwrap();
    fs::write(
        root.path().join("build.json"),
        r#"{"openpilot":{"git_commit":"different"}}"#,
    )
    .unwrap();
    assert!(manifest.verify_identity(root.path()).is_err());
}

#[test]
fn package_cannot_omit_an_installed_daemon_from_its_hash_list() {
    let (root, manifest) = fixture();
    let binaries = root.path().join("rust/bin");
    fs::create_dir_all(&binaries).unwrap();
    fs::write(binaries.join("unlisted-daemon"), b"unverified native bytes").unwrap();
    assert!(manifest.verify(root.path(), &["bin/service"]).is_err());
}
