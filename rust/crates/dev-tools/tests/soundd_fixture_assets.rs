//! Actual native fixture command versus unchanged Python assets() byte captures.
#![cfg(feature = "soundd-policy-native")]
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
fn original() -> Value {
    let bytes = include_bytes!("fixtures/soundd-policy/assets-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "8b0d9e649ca82f00ea12dfb782b7e700b9ab47db9176a94dc8d1345989a16ad1"
    );
    serde_json::from_slice(bytes).unwrap()
}
fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_create-soundd-fixture-assets")
}
fn command(root: &Path, rate: u32, channels: u16) -> Output {
    Command::new(binary())
        .arg(root)
        .arg(rate.to_string())
        .arg(channels.to_string())
        .output()
        .unwrap()
}
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|bytes| u8::from_str_radix(std::str::from_utf8(bytes).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn all_twelve_wave_files_match_original_for_both_channels_and_all_caller_rates() {
    let original = original();
    assert_eq!(original["cases"].as_array().unwrap().len(), 6);
    for case in original["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("missing/nested root");
        let output = command(
            &root,
            case["rate"].as_u64().unwrap() as u32,
            case["channels"].as_u64().unwrap() as u16,
        );
        assert!(output.status.success(), "{case}: {:?}", output.stderr);
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        let actual = files(&root);
        let expected = case["files"].as_object().unwrap();
        assert_eq!(actual.len(), 12);
        assert_eq!(actual.len(), expected.len());
        for (name, row) in expected {
            assert_eq!(actual[name], unhex(row["hex"].as_str().unwrap()), "{name}");
            assert_eq!(
                format!("{:x}", Sha256::digest(&actual[name])),
                row["sha256"].as_str().unwrap()
            );
        }
    }
}
#[test]
fn preexisting_inputs_keep_original_failure_effects_and_bytes() {
    let original = original();
    for row in original["failure_cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("root");
        let name = row["name"].as_str().unwrap();
        if name == "root-file" {
            fs::write(&root, b"preserved root sentinel").unwrap();
        } else {
            let folder = root.join("openpilot/selfdrive/assets/sounds_eng");
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("prompt.wav"), b"preserved original sentinel").unwrap();
        }
        let output = command(&root, 48000, 1);
        assert_eq!(output.status.code(), Some(1), "{name}");
        assert!(output.stdout.is_empty());
        if name == "root-file" {
            assert_eq!(
                fs::read(&root).unwrap(),
                unhex(row["root_bytes_hex"].as_str().unwrap())
            );
        } else {
            let actual = files(&root);
            assert_eq!(actual.len(), row["files"].as_object().unwrap().len());
            for (name, file) in row["files"].as_object().unwrap() {
                assert_eq!(actual[name], unhex(file["hex"].as_str().unwrap()));
            }
        }
    }
}
#[test]
fn explicit_new_cli_contract_rejects_usage_before_filesystem_changes_and_accepts_raw_root() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("must remain absent");
    for args in [
        vec![],
        vec![root.as_os_str().to_owned()],
        vec![root.as_os_str().to_owned(), "48000".into()],
        vec![
            root.as_os_str().to_owned(),
            "48000".into(),
            "1".into(),
            "extra".into(),
        ],
        vec![root.as_os_str().to_owned(), "bad".into(), "1".into()],
        vec![root.as_os_str().to_owned(), "0".into(), "1".into()],
        vec![root.as_os_str().to_owned(), "-1".into(), "1".into()],
        vec![root.as_os_str().to_owned(), "48000".into(), "0".into()],
        vec![root.as_os_str().to_owned(), "48000".into(), "3".into()],
        vec![root.as_os_str().to_owned(), "4294967295".into(), "2".into()],
    ] {
        let output = Command::new(binary()).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(!root.exists());
    }
    let output = Command::new(binary()).arg("--help").output().unwrap();
    assert!(output.status.success());
    assert!(!root.exists());
    use std::os::unix::ffi::OsStringExt;
    let raw = root.with_file_name(std::ffi::OsString::from_vec(b"raw-\xff".to_vec()));
    let output = command(&raw, 48000, 1);
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(files(&raw).len(), 12);
}
