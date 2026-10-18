//! Original entrypoint admission and actual empty-child failure side effects.
#![cfg(feature = "lagd-tools-native")]
use openpilot_dev_tools::{
    lagd_check::{cli_with, Kind},
    qa_process::NativeRunner,
};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, path::Path};
#[path = "support/lagd_cli_original.rs"]
mod expected;
fn files(root: &Path, at: &Path) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(at).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rows.extend(files(root, &path));
        } else {
            rows.push((
                path.strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            ));
        }
    }
    rows.sort();
    rows
}
#[test]
fn original_28_batch_cli_args_and_real_empty_output_effects() {
    let original = expected::cli();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut count = 0;
    for row in original["cases"].as_array().unwrap() {
        let kind = match row["tool"].as_str().unwrap() {
            "numeric" => Kind::Numeric,
            "loop" => Kind::Loop,
            "cache" => Kind::Cache,
            "packet" => Kind::Packet,
            _ => continue,
        };
        count += 1;
        let temporary = tempfile::tempdir().unwrap();
        let args = row["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                OsString::from(
                    v.as_str()
                        .unwrap()
                        .replace("@CASE@", temporary.path().to_str().unwrap()),
                )
            })
            .collect::<Vec<_>>();
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let result = cli_with(
            kind,
            &args,
            &root,
            &mut NativeRunner,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(
            result,
            row["returncode"].as_i64().unwrap() as i32,
            "{} {}: {}",
            row["tool"],
            row["case"],
            String::from_utf8_lossy(&stderr)
        );
        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            row["stdout"].as_str().unwrap(),
            "{} {} stdout",
            row["tool"],
            row["case"]
        );
        if result == 0 {
            assert!(stderr.is_empty());
        } else {
            assert!(!stderr.is_empty());
        }
        let actual = files(temporary.path(), temporary.path());
        let mut want = row["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v["sha256"].as_str().unwrap().to_owned()))
            .collect::<Vec<_>>();
        want.sort();
        assert_eq!(actual, want, "{} {} files", row["tool"], row["case"]);
    }
    assert_eq!(count, 28);
}
