use openpilot_calibrationd::{wire, Calibrator, Limits, Seed};
use openpilot_cereal::log_capnp::event;

#[test]
fn saved_values_round_through_source_float32_and_not_car_overrides_only_packet_fields() {
    let seed = Seed {
        rpy: vec![0.0001, 0.001, 0.002],
        height: vec![1.22],
        valid_blocks: 5,
        ..Seed::default()
    };
    let mut calibrator = Calibrator::new(Limits::standard(), seed).unwrap();
    let saved = wire::encode(&calibrator, 42, true).unwrap();
    let (seed, error) = wire::saved(&saved);
    assert!(error.is_none());
    assert_eq!(seed.rpy[1], f64::from(0.001_f32));
    assert_eq!(seed.height[0], f64::from(1.22_f32));
    calibrator.not_car = true;
    let bytes = wire::encode(&calibrator, 99, false).unwrap();
    let reader = capnp::serialize::read_message(bytes.as_slice(), Default::default()).unwrap();
    let message = reader.get_root::<event::Reader<'_>>().unwrap();
    assert!(!message.get_valid());
    assert_eq!(message.get_log_mono_time(), 99);
    let event::LiveCalibration(data) = message.which().unwrap() else {
        panic!("wrong service")
    };
    let data = data.unwrap();
    assert_eq!(data.get_valid_blocks(), 5);
    assert_eq!(data.get_cal_perc(), 100);
    assert_eq!(
        data.get_rpy_calib().unwrap().iter().collect::<Vec<_>>(),
        vec![0.0; 3]
    );
    assert_eq!(calibrator.rpy[1], 0.001);
}

#[test]
fn malformed_saved_message_reports_error_and_uses_defaults() {
    let (seed, error) = wire::saved(b"not a capnp packet");
    assert!(error.is_some());
    assert_eq!(seed.rpy, vec![0.0; 3]);
    assert_eq!(seed.valid_blocks, 0);
}
