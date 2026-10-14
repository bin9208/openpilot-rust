use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

fn package() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("rust/bin")).unwrap();
    let names = [
        "openpilot-launch",
        "openpilot-runtime-install",
        "openpilot-manager",
        "openpilot-process-child",
        "openpilot-carrot-recovery",
        "openpilot-carrot-web-watchdog",
        "openpilot-carrot-server",
        "openpilot-agnos",
        "openpilot-agnos-updater",
        "openpilot-text-window",
        "openpilot-usbgpu-model",
    ];
    let mut files = Vec::new();
    for name in names {
        let relative = format!("rust/bin/{name}");
        fs::write(root.path().join(&relative), name).unwrap();
        fs::set_permissions(
            root.path().join(&relative),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        files.push(serde_json::json!({"path":relative,"sha256":format!("{:x}",Sha256::digest(name.as_bytes())),"executable":true}));
    }
    fs::write(
        root.path().join("build.json"),
        serde_json::json!({"openpilot":{"git_commit":"1".repeat(40)}}).to_string(),
    )
    .unwrap();
    fs::write(root.path().join("rust/runtime-manifest.json"), serde_json::json!({"schema":1,"source_commit":"1".repeat(40),"agnos_version":"19.8-carrot-bt1","files":files}).to_string()).unwrap();
    root
}

#[test]
fn package_check_returns_without_starting_runtime_children() {
    let root = package();
    let result = Command::new(env!("CARGO_BIN_EXE_openpilot-launch"))
        .arg("--verify-package")
        .arg("--root")
        .arg(root.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!root.path().join("data").exists());
}

#[test]
fn package_check_rejects_corruption() {
    let root = package();
    fs::write(root.path().join("rust/bin/openpilot-manager"), "bad").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_openpilot-launch"))
        .arg("--verify-package")
        .arg("--root")
        .arg(root.path())
        .output()
        .unwrap();
    assert!(!result.status.success());
}
