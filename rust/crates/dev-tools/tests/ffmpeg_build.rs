use std::{fs, process::Command};

#[test]
fn cli_rejects_invalid_arguments_before_creating_build_output() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("output");
    for (program, required) in [
        (
            env!("CARGO_BIN_EXE_build-loggerd-ffmpeg"),
            vec!["--ar", "ar", "--ranlib", "ranlib"],
        ),
        (
            env!("CARGO_BIN_EXE_build-runtime-ffmpeg"),
            vec![
                "--archive",
                "missing.tar.xz",
                "--sdk",
                "missing-sdk",
                "--cxx",
                "c++",
                "--binutils",
                "missing-tools",
            ],
        ),
    ] {
        for extra in [
            vec!["--jobs", "0"],
            vec!["--jobs", "3"],
            vec!["--unknown", "value"],
            vec!["--cc", "duplicate"],
            vec!["--headers-only", "--headers-only"],
        ] {
            let result = Command::new(program)
                .arg("--root")
                .arg(&root)
                .args(["--cc", "cc"])
                .args(&required)
                .args(&extra)
                .output()
                .unwrap();
            assert!(!result.status.success(), "accepted {extra:?}");
            assert!(result.stdout.is_empty());
            assert!(!root.exists(), "created output for {extra:?}");
        }
    }
}

#[test]
fn cli_rejects_unpinned_archive_before_starting_external_tools() {
    let temporary = tempfile::tempdir().unwrap();
    let archive = temporary.path().join("source.tar.xz");
    fs::write(&archive, b"wrong source").unwrap();
    let root = temporary.path().join("output");
    let result = Command::new(env!("CARGO_BIN_EXE_build-loggerd-ffmpeg"))
        .arg("--root")
        .arg(&root)
        .arg("--archive")
        .arg(&archive)
        .args([
            "--cc",
            "must-not-execute",
            "--ar",
            "ar",
            "--ranlib",
            "ranlib",
        ])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("checksum mismatch"));
    assert!(result.stdout.is_empty());
    assert!(!root.join("invocation.json").exists());
}
