//! Hosted only, owned protocol worker and genuine assets; no physical USB GPU.
#![cfg(all(target_os = "linux", feature = "driver-daemon-native"))]
use openpilot_dev_tools::usbgpu_daemon_check::{run, Options};
use std::path::PathBuf;
fn path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} required"))
        .into()
}
#[test]
#[ignore = "hosted six-case original observations, genuine assets and held worker required"]
fn original_disconnect_retry_cancel_lifecycle_is_preserved_at_both_resolutions() {
    let report = run(&Options {
        binary: path("USBGPU_DAEMON_BINARY"),
        worker: path("USBGPU_DAEMON_WORKER"),
        peer: path("USBGPU_DAEMON_PEER"),
        catalog: path("USBGPU_DAEMON_CATALOG"),
        model: path("USBGPU_DAEMON_MODEL"),
        runtime: path("USBGPU_DAEMON_RUNTIME"),
        installed: path("USBGPU_DAEMON_INSTALLED"),
        assets: path("USBGPU_DAEMON_ASSETS"),
        output: path("USBGPU_DAEMON_OUTPUT"),
        original: path("USBGPU_DAEMON_ORIGINAL"),
        original_sha256: std::env::var("USBGPU_DAEMON_ORIGINAL_SHA256").unwrap(),
        root: path("USBGPU_DAEMON_ROOT"),
        expect_absent: std::env::var("MODEL_FAMILY_SOURCE_POLICY").unwrap()
            == "retired-model-family",
        modes: Vec::new(),
    })
    .unwrap();
    let runs = report["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 6);
    for run in runs {
        assert_eq!(run["passed"], true);
        assert_eq!(run["workers_reaped_and_shared_removed"], true);
        if run["mode"] == "cancel" {
            assert_eq!(run["packets"].as_array().unwrap().len(), 0)
        } else {
            assert_eq!(run["packets"].as_array().unwrap().len(), 2)
        }
    }
}
