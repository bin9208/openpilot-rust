use openpilot_dev_tools::registration_vendor_check;
use serde_json::Value;
use std::{
    ffi::OsString,
    fs,
    io::Read,
    os::unix::{ffi::OsStringExt, fs::symlink},
};
fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|x| u8::from_str_radix(std::str::from_utf8(x).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn thirty_seven_original_archive_and_filesystem_cases() {
    let original: Value =
        serde_json::from_str(include_str!("fixtures/registration-vendor/original.json")).unwrap();
    for row in original["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let vendor = root.join("vendor");
        let package = vendor.join("charset-norm");
        fs::create_dir_all(&package).unwrap();
        for item in row["actual"].as_array().unwrap() {
            let p = package.join(OsString::from_vec(bytes(
                item["path_hex"].as_str().unwrap(),
            )));
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, bytes(item["data"].as_str().unwrap())).unwrap();
        }
        for name in row["dirs"].as_array().unwrap() {
            fs::create_dir_all(package.join(name.as_str().unwrap())).unwrap();
        }
        let owned = root.join("owned");
        fs::write(&owned, b"original\0\xff\n").unwrap();
        for (name, target) in row["symlinks"].as_object().unwrap() {
            let p = package.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            let target = target.as_str().unwrap();
            if target == "$OWNED" {
                symlink(&owned, p).unwrap();
            } else {
                symlink(target, p).unwrap();
            }
        }
        fs::write(
            vendor.join("charset-norm-provenance.json"),
            row["provenance"].as_str().unwrap(),
        )
        .unwrap();
        let compressed = bytes(row["archive_gzip_hex"].as_str().unwrap());
        let mut data = vec![];
        flate2::read::GzDecoder::new(compressed.as_slice())
            .read_to_end(&mut data)
            .unwrap();
        let archive = root.join("archive");
        fs::write(&archive, data).unwrap();
        let output = root.join(if row["case"] == "output-parent-missing" {
            "missing/output"
        } else {
            "output"
        });
        let mut printed = vec![];
        let result = registration_vendor_check::check(&vendor, &archive, &output, &mut printed);
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{}: {result:?}",
            row["case"]
        );
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            row["printed"].as_str().unwrap(),
            "{}",
            row["case"]
        );
        assert_eq!(output.exists(), !row["output"].is_null(), "{}", row["case"]);
        if output.exists() {
            assert_eq!(
                fs::read_to_string(output).unwrap(),
                row["output"].as_str().unwrap(),
                "{}",
                row["case"]
            );
        }
    }
}
#[test]
fn cli_help_and_argument_failures_preserve_exit_classes() {
    let temp = tempfile::tempdir().unwrap();
    for (args, status) in [
        (vec!["--help"], 0),
        (vec![], 2),
        (vec!["one"], 2),
        (vec!["--unknown"], 2),
        (vec!["absent", "output"], 1),
    ] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-registration-vendor"))
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(status), "{result:?}");
        assert!(!temp.path().join("output").exists());
    }
}
