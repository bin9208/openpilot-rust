use serde_json::Value;
use std::{
    ffi::OsString,
    fs,
    os::unix::{
        ffi::OsStringExt,
        fs::{symlink, PermissionsExt},
    },
    path::PathBuf,
    process::Command,
};

fn bytes(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn path(value: &Value) -> PathBuf {
    OsString::from_vec(bytes(value.as_str().unwrap())).into()
}

#[test]
fn original_sysroot_selection_hashes_and_failure_publication_without_python() {
    let source: Value =
        serde_json::from_str(include_str!("fixtures/miri-provenance/original.json")).unwrap();
    for case in source["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        for entry in case["entries"].as_array().unwrap() {
            let file = temp.path().join(path(&entry["path_hex"]));
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            match entry["kind"].as_str().unwrap() {
                "file" => fs::write(file, bytes(entry["data_hex"].as_str().unwrap())).unwrap(),
                "dir" => fs::create_dir_all(file).unwrap(),
                "link" => symlink(path(&entry["target_hex"]), file).unwrap(),
                other => panic!("unknown fixture kind {other}"),
            }
        }
        for entry in case["entries"].as_array().unwrap() {
            if let Some(mode) = entry["mode"].as_u64() {
                fs::set_permissions(
                    temp.path().join(path(&entry["path_hex"])),
                    fs::Permissions::from_mode(u32::try_from(mode).unwrap()),
                )
                .unwrap();
            }
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_hash-miri-core"));
        command
            .current_dir(temp.path())
            .env("PATH", "")
            .env_remove("MIRI_SYSROOT");
        if let Some(root) = case["root"].as_str() {
            command.env(
                "MIRI_SYSROOT",
                root.replace("{ROOT}", temp.path().to_str().unwrap()),
            );
        }
        let result = command.output().unwrap();
        for entry in case["entries"].as_array().unwrap() {
            if entry.get("mode").is_some() {
                fs::set_permissions(
                    temp.path().join(path(&entry["path_hex"])),
                    fs::Permissions::from_mode(0o700),
                )
                .unwrap();
            }
        }
        assert_eq!(
            result.status.success(),
            case["status"] == 0,
            "{}: {result:?}",
            case["name"]
        );
        let stdout = String::from_utf8(result.stdout)
            .unwrap()
            .replace(temp.path().to_str().unwrap(), "{ROOT}");
        assert_eq!(
            stdout.as_bytes(),
            bytes(case["stdout_hex"].as_str().unwrap()),
            "{}",
            case["name"]
        );
        assert_eq!(
            result.stderr.is_empty(),
            case["status"] == 0,
            "{}",
            case["name"]
        );
    }
}

#[test]
fn miri_workflow_hashes_actual_sysroot_before_all_original_memory_checks() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow = openpilot_dev_tools::ci_policy::parse(
        &fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap(),
    )
    .unwrap();
    let steps = workflow["jobs"]["radar-memory"]["steps"]
        .as_array()
        .unwrap();
    assert!(!steps.iter().any(|step| step["uses"]
        .as_str()
        .is_some_and(|value| value.starts_with("actions/setup-python"))));
    let run = steps
        .iter()
        .find(|step| {
            step["name"]
                == "Verify BLAS and SVD buffer calls and scheduler ABI at all four Miri levels"
        })
        .unwrap()["run"]
        .as_str()
        .unwrap();
    let setup = run.find("export MIRI_SYSROOT=\"$(cat").unwrap();
    let guard = run.find("--growth-mib 512").unwrap();
    let build = run
        .find("cargo build -p openpilot-dev-tools --bin hash-miri-core --locked")
        .unwrap();
    let hashes = run.find("\"$CARGO_TARGET_DIR/debug/hash-miri-core\" > \"$RUNNER_TEMP/radar-memory/core-hashes.json\"").unwrap();
    let checks = run
        .find("for radar_level in default strict preemption tree")
        .unwrap();
    assert!(setup < guard && guard < build && build < hashes && hashes < checks);
    assert!(!run.contains("python"));
    assert!(run.contains("numerics_ scheduler_argument_abi_preserves_source_fifo_priority"));
    assert!(steps
        .iter()
        .any(|step| step["uses"] == "actions/upload-artifact@v6"
            && step["if"] == "always()"
            && step["with"]["path"]
                .as_str()
                .is_some_and(|path| path.contains("${{ runner.temp }}/radar-memory/"))));
}

#[test]
fn absolute_sysroot_remains_readable_after_current_directory_is_removed() {
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/miri-provenance/deleted-cwd-original.json"
    ))
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("root/lib/rustlib/host/lib/libcore-a.rlib");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, bytes(original["library_bytes_hex"].as_str().unwrap())).unwrap();
    let deleted = temp.path().join("deleted");
    fs::create_dir(&deleted).unwrap();
    let result = Command::new("/bin/sh")
        .arg("-c")
        .arg("/usr/bin/rmdir -- \"$2\" && exec \"$1\"")
        .arg("miri-cwd-probe")
        .arg(env!("CARGO_BIN_EXE_hash-miri-core"))
        .arg(&deleted)
        .current_dir(&deleted)
        .env("PATH", "/nonexistent")
        .env("MIRI_SYSROOT", temp.path().join("root"))
        .output()
        .unwrap();
    assert!(!deleted.exists());
    assert_eq!(
        result.status.code(),
        original["exit"]
            .as_i64()
            .map(|value| i32::try_from(value).unwrap())
    );
    assert_eq!(
        String::from_utf8(result.stdout)
            .unwrap()
            .replace(temp.path().to_str().unwrap(), "{ROOT}"),
        original["stdout"].as_str().unwrap()
    );
    assert_eq!(
        result.stderr,
        original["stderr"].as_str().unwrap().as_bytes()
    );
}
