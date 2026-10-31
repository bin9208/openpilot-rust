use openpilot_runtime_build::inputs::Tree;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    process::Command,
};

#[test]
fn receipt_checks_library_contents_modes_links_and_the_complete_member_set() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("lib")).unwrap();
    fs::write(root.join("lib/provider.so.1"), b"version1").unwrap();
    symlink("provider.so.1", root.join("lib/provider.so")).unwrap();
    let tree = Tree::capture(root).unwrap();
    tree.verify(root).unwrap();
    fs::write(root.join("lib/provider.so.1"), b"version2").unwrap();
    assert!(tree.verify(root).is_err());
    fs::write(root.join("lib/provider.so.1"), b"version1").unwrap();
    let original_mode = fs::metadata(root.join("lib/provider.so.1"))
        .unwrap()
        .permissions()
        .mode();
    fs::set_permissions(
        root.join("lib/provider.so.1"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    assert!(tree.verify(root).is_err());
    fs::set_permissions(
        root.join("lib/provider.so.1"),
        fs::Permissions::from_mode(original_mode),
    )
    .unwrap();
    fs::write(root.join("lib/added.h"), b"unrecorded header").unwrap();
    assert!(tree.verify(root).is_err());
    fs::remove_file(root.join("lib/added.h")).unwrap();
    fs::remove_file(root.join("lib/provider.so")).unwrap();
    assert!(tree.verify(root).is_err());
    symlink("provider.so.1", root.join("lib/provider.so")).unwrap();
    tree.verify(root).unwrap();
}

#[test]
fn escaping_links_directory_links_and_set_id_inputs_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("tree");
    fs::create_dir(&root).unwrap();
    fs::write(temp.path().join("outside"), b"external").unwrap();
    symlink("../outside", root.join("escape")).unwrap();
    assert!(Tree::capture(&root).is_err());
    fs::remove_file(root.join("escape")).unwrap();
    fs::create_dir(root.join("directory")).unwrap();
    symlink("directory", root.join("directory-alias")).unwrap();
    assert!(Tree::capture(&root).is_err());
    fs::remove_file(root.join("directory-alias")).unwrap();
    fs::write(root.join("compiler"), b"input").unwrap();
    fs::set_permissions(root.join("compiler"), fs::Permissions::from_mode(0o4755)).unwrap();
    assert!(Tree::capture(&root).is_err());
}

#[test]
fn real_receipt_cli_round_trip_and_self_inclusion_rejection() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("input");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("header.h"), b"pinned header").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-inputs"))
        .current_dir(temp.path())
        .arg("--root")
        .arg(&root)
        .args(["--output", "receipt.json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = temp.path().join("receipt.json");
    Tree::read(&receipt).unwrap().verify(&root).unwrap();
    let before = fs::read(&receipt).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-inputs"))
        .arg("--root")
        .arg(&root)
        .arg("--output")
        .arg(root.join("self.json"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!root.join("self.json").exists());
    assert_eq!(fs::read(receipt).unwrap(), before);
}
