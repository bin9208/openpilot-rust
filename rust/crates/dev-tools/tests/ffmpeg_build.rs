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

#[test]
fn released_header_cli_rejects_mixed_or_changed_profile_before_preparing_inputs() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let store = temp.path().join("prefixes");
    let mut plan: serde_json::Value =
        serde_json::from_str(include_str!("../../../native-ffmpeg-headers.json")).unwrap();
    plan["target"] = "x86_64-unknown-linux-gnu".into();
    let path = temp.path().join("changed.json");
    fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
    for extra in [
        vec!["--plan", path.to_str().unwrap()],
        vec!["--cc", "must-not-execute"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_build-runtime-ffmpeg"))
            .arg("--prepare-released-headers")
            .arg("--cache")
            .arg(&cache)
            .arg("--store")
            .arg(&store)
            .args(extra)
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!cache.exists() && !store.exists());
    }
}

#[test]
fn provider_component_cli_requires_independent_descriptor_pin_before_cache_creation() {
    let temp = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_build-runtime-ffmpeg"))
        .arg("--prepare-released-provider")
        .arg("--cache")
        .arg(temp.path().join("cache"))
        .arg("--store")
        .arg(temp.path().join("store"))
        .arg("--descriptor")
        .arg(temp.path().join("not-trusted.json"))
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("independent --descriptor-sha256"));
    assert!(!temp.path().join("cache").exists() && !temp.path().join("store").exists());
}
