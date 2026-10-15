use openpilot_usbgpu::{
    client::Client,
    model_delivery::{failure, smoke},
    Error,
};
use serde_json::json;
use std::{fs, process::Command};

#[test]
#[cfg(feature = "native-skip-miri")]
fn actual_native_worker_failure_carries_progress_before_cleanup_without_opening_gpu() {
    let root = tempfile::tempdir().unwrap();
    let model = root.path().join("model.pkl");
    fs::write(&model, b"invalid fixture; rejected before GPU open").unwrap();
    let worker = env!("CARGO_BIN_EXE_openpilot-usbgpu-worker");
    let mut command = if cfg!(target_arch = "aarch64") {
        if let Ok(runner) = std::env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER") {
            let mut words = runner.split_whitespace();
            let mut command = Command::new(words.next().expect("ARM runner executable"));
            command.args(words).arg(worker);
            command
        } else {
            Command::new(worker)
        }
    } else {
        Command::new(worker)
    };
    command.env("USBGPU_ASSETS_ROOT", root.path().join("missing-assets"));
    let error = match Client::load_command(command, &model, [1344, 760]) {
        Ok(_) => panic!("invalid fixture unexpectedly loaded"),
        Err(error) => error,
    };
    let progress = error.worker_snapshot().expect("typed worker context");
    assert!(progress["pid"].as_u64().unwrap() > 1);
    assert_eq!(progress["stage"], "verify_model");
    assert_eq!(progress["frame"], 0);
    assert!(error.to_string().contains("native model assets"));
    assert!(matches!(error.worker_cause(), Error::Protocol(_)));
}

#[test]
fn worker_receipt_stays_valid_and_bounded_with_unicode_error_and_attempt_identity() {
    let root = tempfile::tempdir().unwrap();
    let model = root.path().join("model.pkl");
    fs::write(&model, b"fixture").unwrap();
    fs::write(
        root.path().join("installed.json"),
        serde_json::to_vec(&json!({
            "pickle":{"sha256":"a".repeat(64)}, "runtime":{"sha256":"b".repeat(64)}
        }))
        .unwrap(),
    )
    .unwrap();
    let worker = json!({"pid":42,"stage":"model_call","frame":100,"stat":"x".repeat(2048)});
    let error = Error::Worker {
        source: Box::new(Error::Protocol("가".repeat(20000))),
        progress: worker.clone(),
    };
    let time = smoke::wall_time().unwrap();
    failure::record_error(&model, &error, "inference", time).unwrap();
    let bytes = fs::read(root.path().join("last_failure.json")).unwrap();
    assert!(bytes.len() <= 32768);
    let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(saved["worker"], worker);
    assert_eq!(saved["pickle_sha256"], "a".repeat(64));
    assert_eq!(failure::worker_since(&model, time), Some(worker));
    assert!(failure::worker_since(&model, time + 1.0).is_none());
    let mut changed = saved;
    changed["boot_id"] = json!("other-boot");
    fs::write(
        root.path().join("last_failure.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert!(failure::worker_since(&model, time).is_none());
}
