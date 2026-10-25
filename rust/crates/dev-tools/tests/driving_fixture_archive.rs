//! Hosted-only archive receipt and publication controls.
#![cfg(all(target_os = "linux", feature = "driving-daemon-native"))]
use openpilot_dev_tools::driving_fixture_archive::{stage, Config};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, os::unix::fs::symlink, path::PathBuf};
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
        driving_daemon_reference::fixture::Fixture,
        driving_fixture_archive::MANIFEST_SHA256,
        qa_process::{CommandSpec, NativeRunner, Runner},
    };
    // These mandatory paths publish the one actually verified materialization
    // for following hosted retirement lanes. Never write a receipt inside it.
    let output = PathBuf::from(
        std::env::var_os("DRIVING_FIXTURE_DIRECTORY")
            .expect("mandatory persistent hosted fixture output"),
    );
    let receipt = PathBuf::from(
        std::env::var_os("DRIVING_FIXTURE_STAGE_RECEIPT")
            .expect("mandatory persistent hosted stage receipt"),
    );
    assert!(output.is_absolute() && receipt.is_absolute());
    let receipt_parent = receipt.parent().unwrap();
    assert!(fs::symlink_metadata(receipt_parent).unwrap().is_dir());
    let canonical_output = output
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        .join(output.file_name().unwrap());
    let canonical_receipt = receipt_parent
        .canonicalize()
        .unwrap()
        .join(receipt.file_name().unwrap());
    assert!(!canonical_receipt.starts_with(&canonical_output));
    assert!(!canonical_output.starts_with(&canonical_receipt));
    assert_eq!(
        fs::symlink_metadata(&receipt).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let mut config = Config {
        compact_images: match std::env::var("DRIVING_FIXTURE_ARCHIVE_COMPACT").as_deref() {
            Ok("true") => true,
            Ok("false") => false,
            _ => panic!("explicit DRIVING_FIXTURE_ARCHIVE_COMPACT=true|false required"),
        },
        archive: std::env::var_os("DRIVING_FIXTURE_ARCHIVE")
            .expect("mandatory hosted complete fixture archive")
            .into(),
        output,
        blob_sha256: std::env::var("DRIVING_FIXTURE_ARCHIVE_SHA256")
            .expect("mandatory independently pinned archive SHA256"),
        blob_bytes: std::env::var("DRIVING_FIXTURE_ARCHIVE_BYTES")
            .expect("mandatory independently pinned archive bytes")
            .parse()
            .unwrap(),
    };
    let mut argv = vec![
        std::ffi::OsString::from(env!("CARGO_BIN_EXE_stage-driving-daemon-fixture")),
        "--archive".into(),
        config.archive.as_os_str().to_owned(),
        "--output".into(),
        config.output.as_os_str().to_owned(),
        "--blob-sha256".into(),
        config.blob_sha256.clone().into(),
        "--blob-bytes".into(),
        config.blob_bytes.to_string().into(),
    ];
    if config.compact_images {
        argv.push("--compact-images".into());
    }
    let mut command = CommandSpec::new_os(argv, std::time::Duration::from_secs(20 * 60));
    command.stdout_limit = 1024 * 1024;
    command.stderr_limit = 1024 * 1024;
    let child = NativeRunner
        .run(&command)
        .expect("hosted fixture stager CLI failed to execute");
    assert_eq!(
        child.code,
        0,
        "fixture stager stderr: {}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(
        child.stderr.is_empty(),
        "successful stager emitted stderr: {}",
        String::from_utf8_lossy(&child.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&child.stdout)
        .expect("stager stdout must be one complete JSON receipt");
    assert_eq!(report["historical_programs_executed"], false);
    assert_eq!(report["blob_sha256"], config.blob_sha256);
    assert_eq!(report["blob_bytes"], config.blob_bytes);
    assert_eq!(report["compact_images"], config.compact_images);
    assert_eq!(
        report["generated_images"].as_array().unwrap().len(),
        if config.compact_images { 86 } else { 0 }
    );
    let fixture = Fixture::open(&config.output, MANIFEST_SHA256).unwrap();
    fixture.verify().unwrap();
    assert_eq!(report["output"], config.output.to_str().unwrap());
    assert_eq!(report["fixture_manifest_sha256"], MANIFEST_SHA256);
    let mut published_receipt = tempfile::NamedTempFile::new_in(receipt_parent).unwrap();
    // Retain the CLI's exact stdout, not a reconstructed library receipt.
    published_receipt.write_all(&child.stdout).unwrap();
    published_receipt.persist_noclobber(&receipt).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&receipt).unwrap()).unwrap(),
        report
    );
    assert_eq!(fs::read(&receipt).unwrap(), child.stdout);

    // Keep adversarial files away from the persistent historical fixture.
    let foreign = tempfile::tempdir().unwrap();
    config.output = foreign.path().join("foreign");
    fs::create_dir(&config.output).unwrap();
    fs::write(config.output.join("foreign-receipt"), b"preserve").unwrap();
    assert!(stage(&config).is_err());
    assert_eq!(
        fs::read(config.output.join("foreign-receipt")).unwrap(),
        b"preserve"
    );
    fixture.verify().unwrap();
}
