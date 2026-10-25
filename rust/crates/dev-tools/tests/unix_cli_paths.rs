use std::{
    ffi::OsString,
    fs,
    os::unix::{
        ffi::OsStringExt,
        fs::{symlink, PermissionsExt},
    },
};
#[test]
fn four_original_control_raw_path_cases_match_complete_native_artifacts() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/unix-cli/control-original.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let axis = row["axis"].as_str().unwrap();
        let path = |name: &str| {
            temp.path().join(OsString::from_vec(
                [name.as_bytes(), if name == axis { b"-\xff" } else { b"" }].concat(),
            ))
        };
        let source = path("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("input.json"), b"[]").unwrap();
        fs::write(source.join("expected.json"), b"[]").unwrap();
        let binary = path("binary");
        let real = if axis == "resolved-binary" {
            temp.path().join(OsString::from_vec(b"peer-\xff".to_vec()))
        } else {
            binary.clone()
        };
        fs::write(&real, b"#!/bin/sh\ncat >/dev/null\nprintf \"[]\"\n").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o755)).unwrap();
        if real != binary {
            symlink(&real, &binary).unwrap();
        }
        let output = path("output");
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-control-tools-policy"))
            .arg("--binary")
            .arg(&binary)
            .arg("--source")
            .arg(&source)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(0),
            "{axis}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let wanted = |key: &str| {
            row[key]
                .as_str()
                .unwrap()
                .replace("BASE", temp.path().to_str().unwrap())
        };
        assert_eq!(result.stdout, wanted("printed").as_bytes(), "{axis}");
        assert!(result.stderr.is_empty());
        assert_eq!(
            fs::read(output.join("receipt.json")).unwrap(),
            wanted("receipt").as_bytes(),
            "{axis}"
        );
        assert_eq!(fs::read(output.join("stdout.json")).unwrap(), b"[]");
        assert!(fs::read(output.join("stderr.log")).unwrap().is_empty());
        assert_eq!(fs::read_dir(output).unwrap().count(), 3);
    }
}
#[test]
fn registration_cli_accepts_raw_archive_and_output_as_paths_before_io_error() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp
        .path()
        .join(OsString::from_vec(b"missing-\xff".to_vec()));
    let output = temp
        .path()
        .join(OsString::from_vec(b"output-\xff".to_vec()));
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-registration-vendor"))
        .arg(&archive)
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(!output.exists());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("invalid unicode"));
}
