use openpilot_cereal::log_capnp::event;
use openpilot_dmonitoringmodeld::driver::{driver_transform, Calibration};

fn message(values: &[f32], valid: bool) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_valid(valid);
    let mut rpy = event
        .init_live_calibration()
        .init_rpy_calib(values.len() as u32);
    for (index, value) in values.iter().enumerate() {
        rpy.set(index as u32, *value);
    }
    capnp::serialize::write_message_to_words(&message)
}

#[test]
fn retains_calibration_and_preserves_source_validity_policy() {
    let mut calibration = Calibration::default();
    assert_eq!(calibration.values(), [0.0; 3]);
    calibration
        .update(&message(&[0.1, -0.2, 0.3], false))
        .unwrap();
    assert_eq!(calibration.values(), [0.1, -0.2, 0.3]);
    calibration.update(&message(&[0.5], true)).unwrap();
    assert_eq!(calibration.values(), [0.5; 3]);
}

#[test]
fn rejects_malformed_calibration_without_partial_update() {
    let mut calibration = Calibration::default();
    calibration
        .update(&message(&[0.1, 0.2, 0.3], true))
        .unwrap();
    for values in [vec![], vec![0.0; 2], vec![0.0; 4]] {
        assert!(calibration.update(&message(&values, true)).is_err());
        assert_eq!(calibration.values(), [0.1, 0.2, 0.3]);
    }
    assert!(calibration.update(&[0, 1, 2]).is_err());
    calibration
        .update(&message(&[f32::NAN, f32::INFINITY, -f32::INFINITY], true))
        .unwrap();
    assert!(calibration.values()[0].is_nan());
    assert_eq!(calibration.values()[1..], [f32::INFINITY, -f32::INFINITY]);
}

#[test]
fn maps_both_camera_centers_into_the_original_driver_crop() {
    for (camera, center) in [
        ([1344, 760], [672.0, 380.0]),
        ([1928, 1208], [964.0, 604.0]),
    ] {
        let transform = driver_transform(camera).unwrap();
        assert_eq!(transform[0] * 720.0 + transform[2], center[0]);
        assert_eq!(transform[4] * 356.0 + transform[5], center[1]);
        assert_eq!(&transform[6..], &[0.0, 0.0, 1.0]);
    }
    assert!(driver_transform([1344, 1208]).is_err());
}
