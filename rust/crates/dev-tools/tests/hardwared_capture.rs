#![cfg(all(target_os = "linux", feature = "hardwared-qa-native"))]

use openpilot_dev_tools::hardwared_capture;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn source_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn private_checkout() -> (tempfile::TempDir, PathBuf) {
    let original = source_root();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("checkout");
    let manifest_path = original
        .join("rust/crates/dev-tools/tests/fixtures/hardwared-original/capture-manifest.json");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    for (name, _) in manifest["source_pins"].as_object().unwrap() {
        let source = original.join(name);
        if !source.exists() {
            continue;
        }
        let destination = root.join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(source, destination).unwrap();
    }
    let evidence = root.join("rust/crates/dev-tools/tests/fixtures/hardwared-original");
    fs::create_dir_all(&evidence).unwrap();
    fs::create_dir_all(root.join("rust/tools")).unwrap();
    fs::copy(&manifest_path, evidence.join("capture-manifest.json")).unwrap();
    fs::copy(
        manifest_path.with_file_name("source.json.gz"),
        evidence.join("source.json.gz"),
    )
    .unwrap();
    (temp, root)
}

fn admitted(root: &Path, retired: bool) -> hardwared_capture::Receipt {
    let evidence = root.join("rust/crates/dev-tools/tests/fixtures/hardwared-original");
    let binary = std::env::current_exe().unwrap();
    hardwared_capture::validate(root, &evidence, &binary, &[], None, retired).unwrap()
}

#[test]
fn fixed_original_hardwared_capture_and_source_closure_are_admitted() {
    let root = source_root();
    let evidence = root.join("rust/crates/dev-tools/tests/fixtures/hardwared-original");
    let binary = PathBuf::from("/usr/bin/true").canonicalize().unwrap();
    let retired = std::env::var_os("HARDWARED_REQUIRE_RETIRED").is_some_and(|value| value == "1");
    let receipt =
        hardwared_capture::validate(&root, &evidence, &binary, &[], None, retired).unwrap();
    receipt.verify().unwrap();
    let report = receipt.report();
    assert_eq!(report["input_steps"], 891);
    assert_eq!(report["expected_observations"], 890);
    let source_count = report["source_pins"].as_array().unwrap().len();
    let absent_count = report["absent_retired_tools"].as_array().unwrap().len();
    assert_eq!(source_count + absent_count, 19);
    assert_eq!(absent_count, if retired { 3 } else { 19 - source_count });
}

#[test]
fn held_source_rejects_same_bytes_rewrite() {
    let (_temp, root) = private_checkout();
    let receipt = admitted(&root, false);
    let path = root.join("openpilot/common/pid.py");
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, bytes).unwrap();
    assert!(receipt.verify().is_err());
}

#[test]
fn retired_reappearance_invalidates_held_receipt() {
    let (_temp, root) = private_checkout();
    for relative in [
        "rust/tools/check_hardwared.py",
        "rust/tools/hardwared_source.py",
        "rust/tools/check_hardwared_daemon.py",
    ] {
        let path = root.join(relative);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    let receipt = admitted(&root, true);
    let destination = root.join("rust/tools/check_hardwared.py");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(&destination, b"reappeared retired source").unwrap();
    assert!(receipt.verify().is_err());
}

#[test]
fn non_elf_executable_is_rejected() {
    let (_temp, root) = private_checkout();
    let evidence = root.join("rust/crates/dev-tools/tests/fixtures/hardwared-original");
    let binary = root.join("bad-elf");
    fs::write(&binary, b"not an ELF").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(hardwared_capture::validate(&root, &evidence, &binary, &[], None, false).is_err());
}

#[test]
fn foreign_elf_requires_an_explicit_host_runner() {
    let (_temp, root) = private_checkout();
    let evidence = root.join("rust/crates/dev-tools/tests/fixtures/hardwared-original");
    let binary = root.join("foreign-elf");
    let foreign_machine = if std::env::consts::ARCH == "x86_64" {
        183u16
    } else {
        62u16
    };
    let mut header = [0u8; 20];
    header[..4].copy_from_slice(b"\x7fELF");
    header[4] = 2;
    header[5] = 1;
    header[6] = 1;
    header[16..18].copy_from_slice(&2u16.to_le_bytes());
    header[18..20].copy_from_slice(&foreign_machine.to_le_bytes());
    fs::write(&binary, header).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(hardwared_capture::validate(&root, &evidence, &binary, &[], None, false).is_err());
}

#[test]
fn private_capture_tampering_is_rejected() {
    let (_temp, root) = private_checkout();
    let capture =
        root.join("rust/crates/dev-tools/tests/fixtures/hardwared-original/source.json.gz");
    let mut bytes = fs::read(&capture).unwrap();
    bytes[0] ^= 1;
    fs::write(capture, bytes).unwrap();
    let evidence = root.join("rust/crates/dev-tools/tests/fixtures/hardwared-original");
    let binary = std::env::current_exe().unwrap();
    assert!(hardwared_capture::validate(&root, &evidence, &binary, &[], None, false).is_err());
}
