use openpilot_dev_tools::seal::{seal, BOOT_BINARIES};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
};

fn fixture() -> (tempfile::TempDir, PathBuf, Value) {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("rust/bin/owned-daemon");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    let mut elf = [0; 64];
    elf[..6].copy_from_slice(b"\x7fELF\x02\x01");
    elf[18..20].copy_from_slice(&183u16.to_le_bytes());
    fs::write(&binary, elf).unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let inventory = json!([{"name":"owned", "ready":true, "argv":[binary], "requirements":[{"path":binary, "executable":true}]}]);
    (dir, binary, inventory)
}
fn seal_fixture(root: &Path, inventory: &Value) -> openpilot_dev_tools::seal::Result<Value> {
    seal(root, inventory, "aarch64", &"1".repeat(40), &[], &[])
}
fn requirement(inventory: &mut Value, value: Value) {
    inventory[0]["requirements"]
        .as_array_mut()
        .unwrap()
        .push(value);
}
#[test]
fn manifest_hashes_all_companions_and_required_firmware() {
    let (dir, binary, mut inv) = fixture();
    let companion = binary.parent().unwrap().join("owned-data");
    fs::create_dir(&companion).unwrap();
    fs::write(companion.join("weights"), b"owned weights").unwrap();
    requirement(&mut inv, json!({"path":companion,"directory":true}));
    let firmware = dir.path().join("panda/board/obj/panda.bin.signed");
    fs::create_dir_all(firmware.parent().unwrap()).unwrap();
    fs::write(&firmware, b"owned firmware fixture").unwrap();
    requirement(&mut inv, json!({"path":firmware,"file":true}));
    let manifest = seal_fixture(dir.path(), &inv).unwrap();
    let disk: Value =
        serde_json::from_slice(&fs::read(dir.path().join("rust/runtime-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(disk, manifest);
    assert_eq!(manifest["files"].as_array().unwrap().len(), 3);
    for item in manifest["files"].as_array().unwrap() {
        assert_eq!(item["sha256"].as_str().unwrap().len(), 64);
    }
    assert_eq!(manifest["files"][0]["executable"], false);
}
#[test]
fn native_manager_argument_encoding_is_strict() {
    use std::os::unix::ffi::OsStrExt;
    let (dir, binary, mut inv) = fixture();
    inv[0]["argv"] = json!([{"Unix":binary.as_os_str().as_bytes()}]);
    assert!(seal_fixture(dir.path(), &inv).is_ok());
    for bad in [
        json!({"Unix":[256]}),
        json!({"Unix":[true]}),
        json!({"Unix":"binary"}),
        json!({"Windows":[]}),
        Value::Null,
    ] {
        inv[0]["argv"] = json!([bad]);
        assert!(seal_fixture(dir.path(), &inv)
            .unwrap_err()
            .to_string()
            .contains("argument encoding"));
    }
}
#[test]
fn missing_empty_and_lfs_required_files_are_rejected() {
    for contents in [
        None,
        Some(b"".as_slice()),
        Some(b"version https://git-lfs.github.com/spec/v1\noid sha256:0000\n".as_slice()),
    ] {
        let (dir, _, mut inv) = fixture();
        let asset = dir.path().join("firmware");
        if let Some(bytes) = contents {
            fs::write(&asset, bytes).unwrap();
        }
        requirement(&mut inv, json!({"path":asset,"file":true}));
        assert!(seal_fixture(dir.path(), &inv)
            .unwrap_err()
            .to_string()
            .contains("required file"));
        assert!(!dir.path().join("rust/runtime-manifest.json").exists());
    }
}
#[test]
fn gpu_companions_and_all_boot_helpers_are_required() {
    let (dir, binary, inv) = fixture();
    let companion = binary.parent().unwrap().join("usbgpu-assets");
    for present in [false, true] {
        if present {
            fs::create_dir(&companion).unwrap();
        }
        assert!(seal(
            dir.path(),
            &inv,
            "aarch64",
            &"1".repeat(40),
            &[],
            &["usbgpu-assets"]
        )
        .is_err());
    }
    fs::write(companion.join("kernel"), b"fixture").unwrap();
    assert!(seal(
        dir.path(),
        &inv,
        "aarch64",
        &"1".repeat(40),
        &[],
        &["usbgpu-assets"]
    )
    .is_ok());
    for name in BOOT_BINARIES {
        fs::copy(&binary, binary.parent().unwrap().join(name)).unwrap();
    }
    for missing in ["openpilot-usbgpu-worker", "openpilot-usbgpu-probe"] {
        let path = binary.parent().unwrap().join(missing);
        fs::remove_file(&path).unwrap();
        assert!(seal(
            dir.path(),
            &inv,
            "aarch64",
            &"1".repeat(40),
            &BOOT_BINARIES,
            &[]
        )
        .unwrap_err()
        .to_string()
        .contains(missing));
        fs::copy(&binary, &path).unwrap();
    }
}
#[test]
fn wrong_architecture_scripts_and_unimplemented_catalog_fail_closed() {
    let (dir, binary, mut inv) = fixture();
    inv.as_array_mut()
        .unwrap()
        .push(json!({"name":"carrot_cluster","enabled":true,"ready":false}));
    assert!(seal_fixture(dir.path(), &inv)
        .unwrap_err()
        .to_string()
        .contains("carrot_cluster"));
    inv.as_array_mut().unwrap().pop();
    let elf = fs::read(&binary).unwrap();
    for bytes in [b"#!/bin/sh\nexit 0\n".to_vec(), {
        let mut e = elf;
        e[18..20].copy_from_slice(&62u16.to_le_bytes());
        e
    }] {
        fs::write(&binary, bytes).unwrap();
        assert!(seal_fixture(dir.path(), &inv).is_err());
    }
    assert!(!dir.path().join("rust/runtime-manifest.json").exists());
}
#[test]
fn aliases_preserve_prior_manifest_and_never_escape_root() {
    let (dir, binary, mut inv) = fixture();
    let real = binary.parent().unwrap().join("provider-real");
    fs::create_dir(&real).unwrap();
    requirement(&mut inv, json!({"path":real,"directory":true}));
    assert!(seal_fixture(dir.path(), &inv)
        .unwrap_err()
        .to_string()
        .contains("empty"));
    let library = real.join("library.so");
    symlink("/etc/passwd", &library).unwrap();
    assert!(seal_fixture(dir.path(), &inv)
        .unwrap_err()
        .to_string()
        .contains("escapes"));
    fs::remove_file(&library).unwrap();
    fs::write(&library, b"provider").unwrap();
    let alias = binary.parent().unwrap().join("provider-alias");
    symlink("provider-real", &alias).unwrap();
    let prior = dir.path().join("rust/runtime-manifest.json");
    fs::write(&prior, "prior generation").unwrap();
    assert!(seal_fixture(dir.path(), &inv)
        .unwrap_err()
        .to_string()
        .contains("directory symlink"));
    assert_eq!(fs::read_to_string(&prior).unwrap(), "prior generation");
    fs::remove_file(alias).unwrap();
    symlink(
        "provider-real/library.so",
        binary.parent().unwrap().join("library-alias.so"),
    )
    .unwrap();
    let result = seal_fixture(dir.path(), &inv).unwrap();
    let rows = result["files"].as_array().unwrap();
    assert_eq!(rows[0]["sha256"], rows[2]["sha256"]);
}
#[test]
fn webjoystick_requires_a_ready_native_implementation_even_when_disabled() {
    let (dir, _, mut inv) = fixture();
    inv.as_array_mut()
        .unwrap()
        .push(json!({"name":"webjoystick","enabled":false,"ready":false}));
    assert!(seal_fixture(dir.path(), &inv).is_err());
    let source = dir.path().join("openpilot/tools/bodyteleop/web.py");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(source, b"legacy optional source").unwrap();
    assert!(seal_fixture(dir.path(), &inv).is_err());
}

#[test]
fn malformed_requirement_schema_preserves_the_prior_manifest() {
    for requirements in [
        json!({"path":"missing-required-file","file":true}),
        json!("invalid"),
        Value::Null,
        json!(["invalid"]),
        json!([{"path":42,"file":true}]),
        json!([{"path":"rust/bin/owned-daemon","file":"true"}]),
        json!([{"path":"rust/bin/owned-daemon","file":null}]),
        json!([{"path":"rust/bin/owned-daemon"}]),
        json!([{"path":"rust/bin/owned-daemon","executable":true,"file":true}]),
        json!([{"path":"rust/bin/owned-daemon","flie":true}]),
    ] {
        let (dir, _, mut inventory) = fixture();
        inventory[0]["requirements"] = requirements.clone();
        let manifest = dir.path().join("rust/runtime-manifest.json");
        fs::write(&manifest, b"prior generation").unwrap();
        assert!(
            seal_fixture(dir.path(), &inventory).is_err(),
            "accepted {requirements}"
        );
        assert_eq!(fs::read(&manifest).unwrap(), b"prior generation");
    }
}
#[test]
fn omitted_and_empty_requirement_lists_keep_the_default_contract() {
    let (dir, _, mut inventory) = fixture();
    inventory[0].as_object_mut().unwrap().remove("requirements");
    seal_fixture(dir.path(), &inventory).unwrap();
    inventory[0]["requirements"] = json!([]);
    seal_fixture(dir.path(), &inventory).unwrap();
}
