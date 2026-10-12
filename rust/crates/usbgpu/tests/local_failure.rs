use openpilot_usbgpu::{
    local_artifact::validation,
    model_delivery::failure::{record, Kind},
};
use std::fs;
#[test]
fn local_failure_receipts_are_model_scoped_and_transient_failure_preserves_source() {
    let root = tempfile::tempdir().unwrap();
    let path = root
        .path()
        .join("big_driving_aaaaaaaaaaaaaaaa_tinygrad.pkl");
    fs::write(&path, b"owned-failure-record-input").unwrap();
    let other = root
        .path()
        .join("big_driving_bbbbbbbbbbbbbbbb_tinygrad.pkl.native_boot_validation.json");
    fs::write(&other, b"other-model-receipt").unwrap();
    validation::save(&path, "owned-provider").unwrap();
    assert!(validation::cached(&path, "owned-provider"));
    assert!(!record(&path, "pcie link not up", Kind::Detail, "load", 10.).unwrap());
    assert!(!validation::cached(&path, "owned-provider"));
    assert!(!root
        .path()
        .join("big_driving_aaaaaaaaaaaaaaaa_tinygrad.pkl.native_rejected")
        .exists());
    assert_eq!(fs::read(&path).unwrap(), b"owned-failure-record-input");
    assert!(record(
        &path,
        "invalid local model output",
        Kind::Detail,
        "inference",
        11.
    )
    .unwrap());
    assert!(root
        .path()
        .join("big_driving_aaaaaaaaaaaaaaaa_tinygrad.pkl.native_rejected")
        .is_file());
    assert_eq!(fs::read(other).unwrap(), b"other-model-receipt");
}
