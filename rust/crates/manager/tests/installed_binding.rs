use openpilot_manager_catalog::{
    binding::{self, Installation},
    catalog, ImportConfig,
};
use std::os::unix::fs::PermissionsExt;

fn installation(root: &std::path::Path) -> Installation {
    Installation {
        root: root.into(),
        binaries: root.join("rust/bin"),
    }
}
fn bound(root: &std::path::Path, name: &str) -> binding::Binding {
    let entry = catalog(ImportConfig {
        pc: false,
        tici: true,
        webcam: false,
        carrot_web_external: false,
        darwin: false,
        bodyteleop_available: false,
    })
    .into_iter()
    .find(|entry| entry.name == name)
    .unwrap();
    binding::resolve(entry, &installation(root)).unwrap()
}
fn executable(root: &std::path::Path, name: &str) {
    std::fs::create_dir_all(root.join("rust/bin")).unwrap();
    std::fs::copy("/bin/true", root.join("rust/bin").join(name)).unwrap();
}
#[test]
fn native_card_requires_companion_even_when_executable_exists() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    executable(root, "openpilot-card");
    executable(root, "openpilot-process-child");
    let binding = bound(root, "card");
    assert!(
        matches!(binding.validate(),Err(binding::Error::Directory(path)) if path.ends_with("card-numerics"))
    );
    std::fs::create_dir_all(root.join("rust/bin/card-numerics")).unwrap();
    binding.validate().unwrap();
}
#[test]
fn script_does_not_qualify_as_installed_native_daemon() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    executable(root, "openpilot-process-child");
    let script = root.join("rust/bin/statsd-rs");
    std::fs::write(&script, b"#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(bound(root,"statsd").validate(),Err(binding::Error::Executable(path)) if path == script)
    );
}
#[test]
fn planner_accepts_directory_provider() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    executable(root, "openpilot-plannerd");
    executable(root, "openpilot-process-child");
    std::fs::create_dir_all(root.join("rust/bin/plannerd-acados")).unwrap();
    bound(root, "plannerd").validate().unwrap();
}
