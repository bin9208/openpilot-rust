//! Hosted only: independent eight-case startup evidence and held native peers.
#![cfg(all(target_os = "linux", feature = "driver-daemon-native"))]
use openpilot_dev_tools::model_startup_check::{run, Expect, Options};
use std::path::PathBuf;
fn path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} required"))
        .into()
}
#[test]
#[ignore = "hosted original startup manifest, two native peers and current catalog required"]
fn all_original_startup_cases_are_pre_frame_without_car_params() {
    let report = run(&Options {
        driving: path("MODEL_STARTUP_DRIVING"),
        driver: path("MODEL_STARTUP_DRIVER"),
        driving_peer: path("MODEL_STARTUP_DRIVING_PEER"),
        driver_peer: path("MODEL_STARTUP_DRIVER_PEER"),
        catalog: path("MODEL_STARTUP_CATALOG"),
        original: path("MODEL_STARTUP_ORIGINAL"),
        original_sha256: std::env::var("MODEL_STARTUP_ORIGINAL_SHA256").unwrap(),
        root: path("MODEL_STARTUP_ROOT"),
        output: path("MODEL_STARTUP_OUTPUT"),
        expect: Expect::PreFrame,
        expect_absent: std::env::var("MODEL_FAMILY_SOURCE_POLICY").unwrap()
            == "retired-model-family",
    })
    .unwrap();
    let rows = report["runs"].as_array().unwrap();
    assert_eq!(rows.len(), 8);
    for row in rows {
        assert_eq!(row["native_loaded_before_car_params"], true);
        assert_eq!(row["native_loaded_without_frame"], true);
        assert_eq!(row["frames_sent_to_native"], 0);
        assert_eq!(row["native_exit"], 0);
    }
}
