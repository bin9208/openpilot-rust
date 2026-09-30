#![cfg(feature = "native-skip-miri")]
use openpilot_calibrationd::parameters::{parse_float, PendingWrites};
use openpilot_params::Params;
use std::{
    fs::File,
    time::{Duration, Instant},
};

#[test]
fn cpp_float_prefix_nul_and_range_rules() {
    assert_eq!(parse_float(b" 0.0001tail").unwrap(), f64::from(0.0001_f32));
    assert_eq!(parse_float(b"0x1.8p2").unwrap(), 6.0);
    assert_eq!(parse_float(b"1\0trash").unwrap(), 1.0);
    assert!(parse_float(b"NAN(payload)").unwrap().is_nan());
    assert!(parse_float(b"1e-99").is_err());
    assert!(parse_float(b" ").is_err());
}

#[test]
fn persistence_enqueue_returns_while_original_params_lock_is_held_and_flushes_on_drop() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    let writer = PendingWrites::new(params).unwrap();
    let lock = File::open(root.path().join(".lock")).unwrap();
    lock.lock().unwrap();
    let started = Instant::now();
    writer.put(vec![1, 2, 3]).unwrap();
    assert!(started.elapsed() < Duration::from_millis(100));
    lock.unlock().unwrap();
    drop(writer);
    assert_eq!(
        std::fs::read(root.path().join("d/CalibrationParams")).unwrap(),
        vec![1, 2, 3]
    );
}
