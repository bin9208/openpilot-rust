use std::{fs, path::Path, process::Command};

#[test]
fn native_sdk_mode_rejects_legacy_compiler_arguments_before_preparation() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let store = temp.path().join("store");
    let result = Command::new(env!("CARGO_BIN_EXE_stage-startup-ui-raylib"))
        .args(["--prepare-native-inputs", "--cxx", "must-not-run"])
        .arg("--cache")
        .arg(&cache)
        .arg("--store")
        .arg(&store)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("invalid, mixed or duplicate"));
    assert!(!cache.exists() && !store.exists());
}

#[test]
fn native_sdk_mode_rejects_a_changed_source_lock_without_downloading() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("uv.lock"), "package=[]").unwrap();
    let cache = temp.path().join("cache");
    let store = temp.path().join("store");
    let result = Command::new(env!("CARGO_BIN_EXE_stage-startup-ui-raylib"))
        .arg("--prepare-native-inputs")
        .arg("--root")
        .arg(temp.path())
        .arg("--cache")
        .arg(&cache)
        .arg("--store")
        .arg(&store)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("raylib lock differs"));
    assert!(!cache.exists() && !store.exists());
}

#[test]
fn ffmpeg_prefix_cannot_accept_the_independently_valid_raylib_profile() {
    let temp = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let store = temp.path().join("store");
    let result = Command::new(env!("CARGO_BIN_EXE_build-runtime-ffmpeg"))
        .arg("--prepare-released-prefix")
        .arg("--header-plan")
        .arg(root.join("native-ffmpeg-headers.json"))
        .arg("--provider-plan")
        .arg(root.join("native-raylib-provider.json"))
        .arg("--descriptor")
        .arg(temp.path().join("does-not-exist.json"))
        .arg("--descriptor-sha256")
        .arg("0".repeat(64))
        .arg("--header-cache")
        .arg(temp.path().join("headers"))
        .arg("--provider-cache")
        .arg(temp.path().join("providers"))
        .arg("--store")
        .arg(&store)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("FFmpeg prefix requires its original FFmpeg"));
    assert!(!store.exists());
}
