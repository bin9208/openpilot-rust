use openpilot_process_supervision::CapturedCommand;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

#[test]
fn inherited_exec_preserves_long_tmpdir_and_private_control_lifetime() {
    if let Some(root) = std::env::var_os("OP_EXEC_TMPDIR_PROBE") {
        probe(PathBuf::from(root));
        return;
    }
    let directory = tempfile::Builder::new()
        .prefix("exec-test-")
        .tempdir_in("/tmp")
        .unwrap();
    let long = directory.path().join("long-tmpdir-".repeat(15));
    fs::create_dir(&long).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "inherited_exec_preserves_long_tmpdir_and_private_control_lifetime",
            "--nocapture",
        ])
        .env("OP_EXEC_TMPDIR_PROBE", directory.path())
        .env("TMPDIR", &long)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(directory.path().join("child-tmpdir")).unwrap(),
        long.as_os_str().as_encoded_bytes()
    );
}

fn probe(root: PathBuf) {
    let wrapper = root.join("launcher");
    fs::write(&wrapper, b"#!/bin/sh\nprintf '%s' \"$1\" > \"$OP_EXEC_TMPDIR_PROBE/control-path\"\nexec \"$OP_REAL_HELPER\" \"$@\"\n").unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = CapturedCommand {
        launcher: wrapper,
        cwd: root.clone(),
        argv: ["/bin/sh", "-c", "printf '%s' \"$TMPDIR\" > child-tmpdir"]
            .into_iter()
            .map(Into::into)
            .collect(),
    }
    .spawn_inherited_with_env(&[(
        "OP_REAL_HELPER".into(),
        env!("CARGO_BIN_EXE_openpilot-process-child").into(),
    )])
    .unwrap();
    let descriptor = PathBuf::from(fs::read_to_string(root.join("control-path")).unwrap());
    let control = descriptor.parent().unwrap().to_owned();
    assert_eq!(control.parent().unwrap(), std::path::Path::new("/tmp"));
    assert_eq!(
        fs::metadata(&control).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(child.process.wait().unwrap().success());
    drop(child);
    assert!(
        !control.exists(),
        "private control directory survives child-owner drop"
    );
}
