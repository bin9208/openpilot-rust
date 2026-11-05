//! Exercise the real launcher/manager handoff without a sealed package.
use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[test]
#[ignore]
fn manager_child() {
    let mut socket =
        UnixStream::connect(std::env::var_os("CARROT_BOOT_LOCK_SOCKET").unwrap()).unwrap();
    let lock = openpilot_process_supervision::receive_boot_lock(&mut socket).unwrap();
    lock.unlock().unwrap();
    fs::write(
        std::env::var_os("USER_STARTUP_MANAGER_RECORD").unwrap(),
        "manager started",
    )
    .unwrap();
}

#[test]
#[ignore]
fn process_child() {
    openpilot_process_supervision::run_child(
        fs::File::open(std::env::var_os("USER_STARTUP_CHILD_REQUEST").unwrap()).unwrap(),
    )
    .unwrap();
}

struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn source_build_starts_manager_without_sealing_and_keeps_explicit_audit_available() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    let binaries = root.join("rust/bin");
    fs::create_dir_all(&binaries).unwrap();
    let executable = std::env::current_exe().unwrap();
    for (name, test, prefix) in [
        ("openpilot-manager", "manager_child", ""),
        (
            "openpilot-process-child",
            "process_child",
            "export USER_STARTUP_CHILD_REQUEST=\"$1\"\n",
        ),
    ] {
        let path = binaries.join(name);
        fs::write(
            &path,
            format!(
                "#!/bin/sh\n{prefix}exec '{}' --exact {test} --ignored --nocapture\n",
                executable.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    for name in [
        "openpilot-usbgpu-model",
        "openpilot-carrot-recovery",
        "openpilot-text-window",
    ] {
        fs::copy("/bin/true", binaries.join(name)).unwrap();
    }
    let paths = temp.path().join("paths.json");
    let record = temp.path().join("manager-started");
    fs::write(
        &paths,
        serde_json::to_vec(&serde_json::json!({"repository":root,
        "data":temp.path().join("data"),"temporary":temp.path().join("tmp"),
        "system":temp.path().join("system")}))
        .unwrap(),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_openpilot-launch"));
    command
        .env("OPENPILOT_USER_SOURCE_BUILD", "1")
        .env("USER_STARTUP_MANAGER_RECORD", &record)
        .env("CARROT_WEB_EXTERNAL", "0")
        .env_remove("CARROT_BOOT_LOCK_SOCKET")
        .env_remove("CARROT_REPO_LOCK_PATH")
        .env_remove("OPENPILOT_NATIVE_CONFIG")
        .env_remove("OPENPILOT_NATIVE_MACHINE_CONFIG")
        .args(["--paths", paths.to_str().unwrap()]);
    let mut child = Running(
        command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !record.is_file() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "launcher exited before manager"
        );
        assert!(
            Instant::now() < deadline,
            "source launcher did not start manager"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!root.join("rust/runtime-manifest.json").exists());
    let audit = Command::new(env!("CARGO_BIN_EXE_openpilot-launch"))
        .env("OPENPILOT_USER_SOURCE_BUILD", "1")
        .args(["--verify-package", "--root", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !audit.status.success(),
        "explicit audit must still reject an absent manifest"
    );
}
