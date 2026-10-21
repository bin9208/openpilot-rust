//! Hosted only: native regeneration, both independent rosters, and atomic output.
#![cfg(all(target_os = "linux", feature = "driver-daemon-native"))]
use openpilot_dev_tools::{
    driver_daemon_check::OriginalCapture,
    driver_daemon_fixture::{
        stage, verify_blob, COLLECTOR_MANIFEST, CURRENT_COLLECTOR_MANIFEST, DIRECTORY,
        NO_COLLECTOR_MANIFEST,
    },
};
use std::{fs, os::unix::fs::symlink, path::Path, process::Command};
#[test]
fn all_original_histories_are_durable_and_current_modes_share_exact_models() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let bytes = fs::read(root.join(DIRECTORY).join("observations.tar.xz")).unwrap();
    let packed = verify_blob(&bytes).unwrap();
    assert_eq!(packed.len(), 97);
    for invalid in [vec![], bytes[..bytes.len() / 2].to_vec(), {
        let mut changed = bytes.clone();
        let n = changed.len() / 2;
        changed[n] ^= 1;
        changed
    }] {
        assert!(verify_blob(&invalid).is_err())
    }
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    let staged = Command::new(env!("CARGO_BIN_EXE_stage-driver-daemon-fixture"))
        .args([
            "--root".as_ref(),
            root.as_os_str(),
            "--output".as_ref(),
            output.as_os_str(),
        ])
        .output()
        .unwrap();
    assert!(
        staged.status.success(),
        "{}",
        String::from_utf8_lossy(&staged.stderr)
    );
    assert!(staged.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&staged.stdout).unwrap();
    assert_eq!(report["regenerated_images"], 42);
    assert_eq!(report["historical_programs_executed"], false);
    assert_eq!(report["collector_data_ready"], true);
    assert_eq!(report["native_retirement_accepted"], false);
    assert_eq!(report["historical_collector_execution_ready"], false);
    assert!(report["historical_collector_models"].is_null());
    assert_eq!(report["collector_models"], report["no_collector_models"]);
    for (kind, digest) in [
        ("collector", COLLECTOR_MANIFEST),
        ("no-collector", NO_COLLECTOR_MANIFEST),
        ("collector-current", CURRENT_COLLECTOR_MANIFEST),
    ] {
        let source = report["datasets"][kind]["source_commit"].as_str().unwrap();
        let original =
            OriginalCapture::open(&output.join(kind).join("observations"), digest, source).unwrap();
        let mut count = 0;
        for (width, raw, frames) in [(1344, 1, 4), (1344, 0, 1), (1928, 1, 4), (1928, 0, 1)] {
            for i in 0..=frames {
                assert!(!original
                    .read(&format!("{width}-raw-{raw}/reference/expected-{i}.bin"))
                    .unwrap()
                    .is_empty());
                count += 1;
            }
        }
        assert_eq!(count, 14);
        original.verify().unwrap();
    }
    fs::write(output.join("foreign"), b"preserve").unwrap();
    assert!(stage(&root, &output).is_err());
    assert_eq!(fs::read(output.join("foreign")).unwrap(), b"preserve");
    let alias = temp.path().join("alias");
    symlink(&output, &alias).unwrap();
    assert!(stage(&root, &alias).is_err());
    let missing = temp.path().join("broken");
    symlink(temp.path().join("absent"), &missing).unwrap();
    assert!(stage(&root, &missing).is_err());
    let aliased_parent = temp.path().join("parent-link");
    symlink(temp.path(), &aliased_parent).unwrap();
    assert!(stage(&root, &aliased_parent.join("unused")).is_err());
}
