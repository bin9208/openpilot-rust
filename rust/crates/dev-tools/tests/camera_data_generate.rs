use openpilot_dev_tools::camera_data_generate::bps;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, process::Command};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn decode(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn original_bps_source_generation_preserves_values_and_partial_failure_output() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/camera-data/bps-original.json")).unwrap();
    for case in original["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("openpilot/system/camerad/cameras/bps_blobs.h");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = if let Some(text) = case["source_hex"].as_str() {
            decode(text)
        } else {
            fs::read(root().join("openpilot/system/camerad/cameras/bps_blobs.h")).unwrap()
        };
        fs::write(&path, bytes).unwrap();
        let mut output = Vec::new();
        let result = bps(directory.path(), &mut output);
        assert_eq!(
            result.is_ok(),
            case["exit"] == 0,
            "{}: {result:?}",
            case["name"]
        );
        assert_eq!(
            output.len() as u64,
            case["stdout_bytes"].as_u64().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&output)),
            case["stdout_sha256"],
            "{}",
            case["name"]
        );
        if let Some(expected) = case["stdout_hex"].as_str() {
            assert_eq!(output, decode(expected), "{}", case["name"]);
        }
    }
}
#[test]
fn bps_native_cli_runs_without_python_and_keeps_argument_failure_class() {
    let binary = env!("CARGO_BIN_EXE_generate-camerad-bps-data");
    let output = Command::new(binary)
        .arg("--source")
        .arg(root())
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let mut expected = Vec::new();
    bps(&root(), &mut expected).unwrap();
    assert_eq!(output.stdout, expected);
    assert_eq!(
        Command::new(binary).output().unwrap().status.code(),
        Some(2)
    );
}

#[test]
fn pinned_python_unicode15_word_and_uppercase_match_exhaustive_original_capture() {
    use openpilot_dev_tools::camera_data_generate::unicode;
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/camera-data/bps-original.json")).unwrap();
    let mut word = Sha256::new();
    let mut upper = Sha256::new();
    let mut decimal = Sha256::new();
    for point in 0..=0x10ffffu32 {
        if let Some(ch) = char::from_u32(point) {
            word.update([u8::from(unicode::word(ch))]);
            decimal.update([u8::from(unicode::decimal(ch))]);
            let value = unicode::uppercase(&ch.to_string());
            upper.update([u8::try_from(value.chars().count()).unwrap()]);
            for ch in value.chars() {
                upper.update(u32::from(ch).to_le_bytes());
            }
        } else {
            // Python surrogate strings are not word characters and upper leaves
            // them unchanged; UTF8 source files cannot contain these codepoints.
            word.update([0]);
            decimal.update([0]);
            upper.update([1]);
            upper.update(point.to_le_bytes());
        }
    }
    assert_eq!(
        format!("{:x}", word.finalize()),
        original["unicode_capture"]["word_sha256"]
    );
    assert_eq!(
        format!("{:x}", decimal.finalize()),
        original["unicode_capture"]["decimal_sha256"]
    );
    assert_eq!(
        format!("{:x}", upper.finalize()),
        original["unicode_capture"]["upper_sha256"]
    );
}
