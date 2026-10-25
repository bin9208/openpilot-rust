use openpilot_dev_tools::release_files;
use serde_json::Value;
use std::{fs, os::unix::fs::symlink, path::PathBuf, process::Command};

#[test]
fn original_release_selection_preserves_regexes_and_symlinks_without_following_them() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/release-files/original.json")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    for name in oracle["files"].as_array().unwrap() {
        let path = temp.path().join(name.as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"fixture\n").unwrap();
    }
    fs::create_dir(temp.path().join("release")).unwrap();
    fs::write(
        temp.path().join("release/release_files.py"),
        b"fixture path only\n",
    )
    .unwrap();
    for (name, target) in oracle["links"].as_object().unwrap() {
        symlink(target.as_str().unwrap(), temp.path().join(name)).unwrap();
    }
    assert!(Command::new("mkfifo")
        .arg(temp.path().join(oracle["fifo"].as_str().unwrap()))
        .status()
        .unwrap()
        .success());
    let mut actual = release_files::list(temp.path()).unwrap();
    let mut expected: Vec<PathBuf> = oracle["expected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| PathBuf::from(v.as_str().unwrap()))
        .collect();
    // pathlib and read_dir expose filesystem iteration order; the release copy
    // contract is the selected path set, including directory and dangling links.
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

#[test]
fn missing_root_and_regular_file_root_have_no_descendants() {
    let temp = tempfile::tempdir().unwrap();
    assert!(release_files::list(&temp.path().join("absent"))
        .unwrap()
        .is_empty());
    let file = temp.path().join("file");
    fs::write(&file, b"file").unwrap();
    assert!(release_files::list(&file).unwrap().is_empty());
}

#[test]
fn shell_bootstrap_reserves_before_build_and_keeps_stdout_as_only_file_names() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    let tools = temp.path().join("tools");
    let cache = temp.path().join("cache");
    fs::create_dir_all(root.join("release")).unwrap();
    fs::create_dir_all(root.join("rust/tools")).unwrap();
    fs::create_dir(&tools).unwrap();
    fs::write(root.join("README"), b"fixture").unwrap();
    let bootstrap = root.join("release/release_files");
    fs::write(
        &bootstrap,
        include_str!("../../../../release/release_files"),
    )
    .unwrap();
    let guard = root.join("rust/tools/check-disk-space");
    fs::write(&guard, "#!/bin/bash\nprintf 'guard\\n' >> \"$TRACE\"\nprintf '{\"free_bytes\":123}\\n'\nexit \"${GUARD_STATUS:-0}\"\n").unwrap();
    let cargo = tools.join("cargo");
    fs::write(
        &cargo,
        r#"#!/bin/bash
set -eu
printf 'cargo\n' >> "$TRACE"
test "$1" = +1.94.0
test -z "${CARGO_BUILD_TARGET+x}${RUSTFLAGS+x}${CARGO_ENCODED_RUSTFLAGS+x}${RUSTC_WRAPPER+x}${RUSTC_WORKSPACE_WRAPPER+x}"
test "${CARGO_INCREMENTAL}" = 0
test "${CARGO_BUILD_JOBS}" = 2
if [ "${CARGO_STATUS:-0}" != 0 ]; then exit "$CARGO_STATUS"; fi
target= cache=
while (($#)); do
  case "$1" in --target) target=$2; shift ;; --target-dir) cache=$2; shift ;; esac
  shift
done
mkdir -p "$cache/$target/debug"
printf '#!/bin/bash\nexec "$REAL_RELEASE_FILES" "$@"\n' > "$cache/$target/debug/release-files"
chmod +x "$cache/$target/debug/release-files"
"#,
    )
    .unwrap();
    for file in [&bootstrap, &guard, &cargo] {
        fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let trace = temp.path().join("trace");
    let run = |status: &str, cache: &std::path::Path| {
        Command::new("/bin/bash")
            .arg(&bootstrap)
            .env("PATH", format!("{}:/usr/bin:/bin", tools.display()))
            .env("OPENPILOT_RELEASE_TOOLS_CACHE", cache)
            .env("TRACE", &trace)
            .env("GUARD_STATUS", status)
            .env("CARGO_BUILD_TARGET", "aarch64-invalid-cross-target")
            .env("RUSTFLAGS", "--invalid-cross-flags")
            .env("RUSTC_WRAPPER", "/must-not-run-wrapper")
            .env("REAL_RELEASE_FILES", env!("CARGO_BIN_EXE_release-files"))
            .output()
            .unwrap()
    };
    let output = run("0", &cache);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&trace).unwrap(), "guard\ncargo\n");
    assert!(String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line == "README"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("free_bytes"));
    fs::remove_file(&trace).unwrap();
    let output = run("42", &cache);
    assert_eq!(output.status.code(), Some(42));
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read_to_string(&trace).unwrap(), "guard\n");
    fs::remove_file(&trace).unwrap();
    assert!(!run("0", &root.join("cache")).status.success());
    assert!(!trace.exists());
    let failed = Command::new("/bin/bash")
        .arg(&bootstrap)
        .env("PATH", format!("{}:/usr/bin:/bin", tools.display()))
        .env("OPENPILOT_RELEASE_TOOLS_CACHE", &cache)
        .env("TRACE", &trace)
        .env("GUARD_STATUS", "0")
        .env("CARGO_STATUS", "43")
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(43));
    assert!(failed.stdout.is_empty());
    assert_eq!(fs::read_to_string(&trace).unwrap(), "guard\ncargo\n");
}
