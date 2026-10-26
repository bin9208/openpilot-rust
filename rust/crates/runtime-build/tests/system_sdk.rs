use openpilot_runtime_build::{
    digest_file,
    inputs::{Receipt, Tree},
    system_sdk::{Entry, Plan, Snapshot},
};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

fn image() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for dir in [
        "usr/include",
        "usr/lib/aarch64-linux-gnu",
        "etc/alternatives",
    ] {
        fs::create_dir_all(temp.path().join(dir)).unwrap();
    }
    for (path, bytes) in [
        ("VERSION", "19.8-carrot-bt1\n"),
        ("usr/include/native.h", "header01"),
        ("usr/lib/aarch64-linux-gnu/libnative.so.1", "library1"),
    ] {
        fs::write(temp.path().join(path), bytes).unwrap();
    }
    symlink("/usr/lib", temp.path().join("lib")).unwrap();
    symlink(
        "../../etc/alternatives/native.h",
        temp.path().join("usr/include/selected.h"),
    )
    .unwrap();
    symlink(
        "/usr/include/native.h",
        temp.path().join("etc/alternatives/native.h"),
    )
    .unwrap();
    temp
}

fn plan() -> Plan {
    Plan {
        schema: 1,
        agnos_version: "19.8-carrot-bt1".into(),
        factory_image_sha256: "1".repeat(64),
        roots: BTreeMap::from([
            (PathBuf::from("lib/aarch64-linux-gnu"), true),
            (PathBuf::from("usr/include"), true),
            (PathBuf::from("usr/local/include"), true),
        ]),
    }
}

#[test]
fn repository_profile_retains_the_verified_factory_image_and_declared_scopes() {
    let profile =
        Plan::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native-system-sdk.json"))
            .unwrap();
    assert_eq!(
        profile.factory_image_sha256,
        "375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44"
    );
    for path in [
        "usr/include",
        "usr/lib/gcc",
        "usr/lib/llvm-18",
        "usr/lib/aarch64-linux-gnu",
    ] {
        assert_eq!(profile.roots.get(Path::new(path)), Some(&true));
    }
    assert_eq!(profile.roots.get(Path::new("usr/lib")), Some(&false));
    assert!(!profile.roots.contains_key(Path::new("usr/lib/python3")));
}

#[test]
fn image_absolute_and_directory_links_never_resolve_against_the_host() {
    let image = image();
    let root = image.path();
    let snapshot = Snapshot::capture(root, &plan()).unwrap();
    snapshot.verify(root).unwrap();
    assert!(matches!(
        snapshot.entries[Path::new("lib")],
        Entry::Symlink { .. }
    ));
    assert!(matches!(
        snapshot.entries[Path::new("usr/lib/aarch64-linux-gnu/libnative.so.1")],
        Entry::File { ref sha256, bytes: 8, .. }
            if *sha256 == digest_file(&root.join("usr/lib/aarch64-linux-gnu/libnative.so.1")).unwrap()
    ));
    assert!(matches!(
        snapshot.entries[Path::new("usr/local")],
        Entry::Absent
    ));
    // The ordinary toolchain/provider receipt deliberately keeps its old policy.
    assert!(Tree::capture(root).is_err());
    fs::remove_file(root.join("lib")).unwrap();
    symlink("usr/lib", root.join("lib")).unwrap();
    assert!(snapshot.verify(root).is_err());
}

#[test]
fn same_size_header_library_modes_and_directory_additions_invalidate_the_receipt() {
    let image = image();
    let root = image.path();
    let snapshot = Snapshot::capture(root, &plan()).unwrap();
    for (path, original, changed) in [
        ("usr/include/native.h", "header01", "header02"),
        (
            "usr/lib/aarch64-linux-gnu/libnative.so.1",
            "library1",
            "library2",
        ),
    ] {
        fs::write(root.join(path), changed).unwrap();
        assert!(snapshot.verify(root).is_err());
        fs::write(root.join(path), original).unwrap();
        snapshot.verify(root).unwrap();
    }
    let path = root.join("usr/include/native.h");
    let mode = fs::metadata(&path).unwrap().permissions().mode();
    fs::set_permissions(&path, fs::Permissions::from_mode((mode & 0o7777) ^ 0o100)).unwrap();
    assert!(snapshot.verify(root).is_err());
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    fs::write(root.join("usr/include/added.h"), "unrecorded").unwrap();
    assert!(snapshot.verify(root).is_err());
    fs::remove_file(root.join("usr/include/added.h")).unwrap();
    fs::create_dir_all(root.join("usr/local/include")).unwrap();
    assert!(snapshot.verify(root).is_err());
}

#[test]
fn shallow_library_namespace_pins_immediate_files_but_not_unselected_subtrees() {
    let image = image();
    let root = image.path();
    fs::write(root.join("usr/lib/direct.so"), "direct01").unwrap();
    fs::create_dir(root.join("usr/lib/python3")).unwrap();
    let mut profile = plan();
    profile.roots.insert("usr/lib".into(), false);
    let snapshot = Snapshot::capture(root, &profile).unwrap();
    fs::write(
        root.join("usr/lib/python3/unrelated.pyc"),
        "outside declared scope",
    )
    .unwrap();
    snapshot.verify(root).unwrap();
    fs::write(root.join("usr/lib/direct.so"), "direct02").unwrap();
    assert!(snapshot.verify(root).is_err());
    fs::write(root.join("usr/lib/direct.so"), "direct01").unwrap();
    fs::create_dir(root.join("usr/lib/new-namespace")).unwrap();
    assert!(snapshot.verify(root).is_err());
}

#[test]
fn unsafe_links_cycles_set_id_special_files_and_wrong_os_fail_closed() {
    let image = image();
    let root = image.path();
    let link = root.join("usr/include/unsafe");
    for target in [
        "/proc/self/maps",
        "../../../usr/include/native.h",
        "/data/secret",
        "loop",
    ] {
        symlink(target, &link).unwrap();
        if target == "loop" {
            symlink("unsafe", root.join("usr/include/loop")).unwrap();
        }
        assert!(Snapshot::capture(root, &plan()).is_err(), "{target}");
        fs::remove_file(&link).unwrap();
        if target == "loop" {
            fs::remove_file(root.join("usr/include/loop")).unwrap();
        }
    }
    let header = root.join("usr/include/native.h");
    fs::set_permissions(&header, fs::Permissions::from_mode(0o4755)).unwrap();
    assert!(Snapshot::capture(root, &plan()).is_err());
    fs::set_permissions(header, fs::Permissions::from_mode(0o644)).unwrap();
    let fifo = root.join("usr/include/pipe");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    assert!(Snapshot::capture(root, &plan()).is_err());
    fs::remove_file(fifo).unwrap();
    fs::write(root.join("VERSION"), "different OS\n").unwrap();
    assert!(Snapshot::capture(root, &plan()).is_err());
}

#[test]
fn typed_receipt_cannot_verify_an_unused_sysroot_or_replace_a_provider_tree() {
    let image = image();
    let snapshot = Snapshot::capture(image.path(), &plan()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let receipt = temp.path().join("sdk.json");
    fs::write(&receipt, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let typed = Receipt::read(&receipt).unwrap();
    assert!(typed.verify(image.path()).is_err());
    assert!(Tree::read(&receipt).is_err());
    let mut invalid = serde_json::to_value(snapshot).unwrap();
    invalid["entries"]["../escape"] = serde_json::json!({"kind":"absent"});
    fs::write(&receipt, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(Receipt::read(&receipt).is_err());
}

#[test]
fn capture_cli_requires_external_output_and_preserves_existing_receipts() {
    let image = image();
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("plan.json");
    let receipt = temp.path().join("sdk.json");
    fs::write(&profile, serde_json::to_vec(&plan()).unwrap()).unwrap();
    let invoke = |output: &Path| {
        Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-inputs"))
            .args(["--capture-system-sdk", "--root"])
            .arg(image.path())
            .arg("--plan")
            .arg(&profile)
            .arg("--output")
            .arg(output)
            .output()
            .unwrap()
    };
    let result = invoke(&receipt);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = fs::read(&receipt).unwrap();
    serde_json::from_slice::<Snapshot>(&bytes)
        .unwrap()
        .verify(image.path())
        .unwrap();
    assert!(!invoke(&receipt).status.success());
    assert_eq!(fs::read(&receipt).unwrap(), bytes);
    let bad_output = image.path().join("self.json");
    assert!(!invoke(&bad_output).status.success());
    assert!(!bad_output.exists());
}
