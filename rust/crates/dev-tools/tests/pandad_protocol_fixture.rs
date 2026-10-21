//! Fixture authentication/staging controls; executed only in hosted Actions.
#![cfg(all(target_os = "linux", feature = "pandad-protocol-native"))]
use openpilot_dev_tools::pandad_protocol_fixture::{
    stage, verify_blob, DIRECTORY, MANIFEST_SHA256,
};
use serde_json::Value;
use std::{fs, os::unix::fs::symlink, path::Path};

#[test]
fn accepted_fixture_is_complete_and_rejects_corruption_sources_links_and_foreign_output() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let blob = fs::read(root.join(DIRECTORY).join("observations.tar.gz")).unwrap();
    let files = verify_blob(&blob).unwrap();
    assert_eq!(files.len(), 319);
    let builder: Value = serde_json::from_slice(&files["builder.json"]).unwrap();
    let sources = builder["sources"].as_object().unwrap();
    assert_eq!(sources.len(), 14);
    for corrupt in [vec![], blob[..blob.len() - 1].to_vec(), {
        let mut bytes = blob.clone();
        bytes[blob.len() / 2] ^= 1;
        bytes
    }] {
        assert!(verify_blob(&corrupt).is_err());
    }
    let temp = tempfile::tempdir().unwrap();
    let copied = temp.path().join("root");
    for name in sources.keys() {
        let path = copied.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(root.join(name), path).unwrap();
    }
    let packed = copied.join(DIRECTORY).join("observations.tar.gz");
    fs::create_dir_all(packed.parent().unwrap()).unwrap();
    fs::write(&packed, &blob).unwrap();
    let output = temp.path().join("output");
    let report = stage(&copied, &output).unwrap();
    assert_eq!(report["capture_manifest_sha256"], MANIFEST_SHA256);
    assert_eq!(report["original_files"], 317);
    assert_eq!(report["historical_programs_executed"], false);
    for (name, bytes) in &files {
        assert_eq!(fs::read(output.join(name)).unwrap(), *bytes, "{name}");
    }
    assert!(output.join("environment.env").is_file());
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(output.join("environment.json")).unwrap())
            .unwrap(),
        report
    );
    fs::write(output.join("foreign"), b"preserve").unwrap();
    assert!(stage(&copied, &output).is_err());
    assert_eq!(fs::read(output.join("foreign")).unwrap(), b"preserve");

    let name = sources.keys().next().unwrap();
    let source = copied.join(name);
    let accepted = fs::read(&source).unwrap();
    fs::write(&source, b"changed source").unwrap();
    let failed = temp.path().join("failed");
    assert!(stage(&copied, &failed).is_err());
    assert!(!failed.exists());
    fs::remove_file(&source).unwrap();
    symlink(root.join(name), &source).unwrap();
    assert!(stage(&copied, &failed).is_err());
    assert!(!failed.exists());
    fs::remove_file(&source).unwrap();
    fs::write(source, accepted).unwrap();
    fs::write(packed, b"corrupt archive").unwrap();
    assert!(stage(&copied, &failed).is_err());
    assert!(!failed.exists());
}
