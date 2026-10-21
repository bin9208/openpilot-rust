//! Hosted-only archive receipt and publication controls.
#![cfg(all(target_os = "linux", feature = "driving-daemon-native"))]
use openpilot_dev_tools::driving_fixture_archive::{stage, Config};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::symlink};
#[test]
fn rejects_bad_receipt_corrupt_archive_symlink_and_preserves_foreign_output() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("archive.gz");
    fs::write(&archive, b"not gzip").unwrap();
    let output = temp.path().join("foreign");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("receipt"), b"preserve").unwrap();
    let mut config = Config {
        archive: archive.clone(),
        output: output.clone(),
        blob_sha256: format!("{:x}", Sha256::digest(b"not gzip")),
        blob_bytes: 8,
        compact_images: false,
    };
    assert!(stage(&config).is_err());
    assert_eq!(fs::read(output.join("receipt")).unwrap(), b"preserve");
    config.output = temp.path().join("absent");
    config.blob_sha256 = "0".repeat(64);
    assert!(stage(&config).is_err());
    assert!(!config.output.exists());
    config.blob_bytes = 513 * 1024 * 1024;
    assert!(stage(&config).is_err());
    config.blob_bytes = 8;
    let link = temp.path().join("link.gz");
    symlink(&archive, &link).unwrap();
    config.archive = link;
    assert!(stage(&config).is_err());
    assert!(!config.output.exists());
    let linked_parent = temp.path().join("linked-parent");
    symlink(temp.path(), &linked_parent).unwrap();
    config.archive = linked_parent.join("archive.gz");
    assert!(stage(&config).is_err());
    assert!(!config.output.exists());
}

#[test]
fn complete_sealed_data_stages_and_foreign_output_is_never_replaced() {
    use openpilot_dev_tools::{
        driving_daemon_reference::fixture::Fixture, driving_fixture_archive::MANIFEST_SHA256,
    };
    let temp = tempfile::tempdir().unwrap();
    let config = Config {
        compact_images: match std::env::var("DRIVING_FIXTURE_ARCHIVE_COMPACT").as_deref() {
            Ok("true") => true,
            Ok("false") => false,
            _ => panic!("explicit DRIVING_FIXTURE_ARCHIVE_COMPACT=true|false required"),
        },
        archive: std::env::var_os("DRIVING_FIXTURE_ARCHIVE")
            .expect("mandatory hosted complete fixture archive")
            .into(),
        output: temp.path().join("staged"),
        blob_sha256: std::env::var("DRIVING_FIXTURE_ARCHIVE_SHA256")
            .expect("mandatory independently pinned archive SHA256"),
        blob_bytes: std::env::var("DRIVING_FIXTURE_ARCHIVE_BYTES")
            .expect("mandatory independently pinned archive bytes")
            .parse()
            .unwrap(),
    };
    let report = stage(&config).unwrap();
    assert_eq!(report["historical_programs_executed"], false);
    Fixture::open(&config.output, MANIFEST_SHA256)
        .unwrap()
        .verify()
        .unwrap();
    fs::write(config.output.join("foreign-receipt"), b"preserve").unwrap();
    assert!(stage(&config).is_err());
    assert_eq!(
        fs::read(config.output.join("foreign-receipt")).unwrap(),
        b"preserve"
    );
}
