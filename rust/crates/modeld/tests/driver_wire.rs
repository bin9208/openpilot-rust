use openpilot_cereal::log_capnp::{driver_state_v2, event};
use openpilot_modeld::{
    driver_wire::{encode, DriverTiming},
    parse::Normal,
    prediction::{DriverData, DriverPrediction},
};

fn prediction() -> DriverPrediction {
    DriverPrediction {
        left: DriverData {
            face: Normal {
                mean: [1.0, 2.0, 3.0, 4.0, 5.0, 999.0],
                std: [0.1, 0.2, 0.3, 0.4, 0.5, 888.0],
            },
            probabilities: [0.11, 0.12, 0.13, 0.14, 0.15, 0.16, 0.17, 0.18],
        },
        right: DriverData {
            face: Normal {
                mean: [-1.0, -2.0, -3.0, -4.0, -5.0, 777.0],
                std: [0.6, 0.7, 0.8, 0.9, 1.0, 666.0],
            },
            probabilities: [0.21, 0.22, 0.23, 0.24, 0.25, 0.26, 0.27, 0.28],
        },
        wheel_on_right: 0.75,
    }
}

fn timing() -> DriverTiming {
    DriverTiming {
        log_mono_time: 1234567890123,
        frame_id: 987,
        model_execution_time: 0.03125,
        gpu_execution_time: 0.015625,
    }
}

fn assert_data(actual: driver_state_v2::driver_data::Reader<'_>, expected: &DriverData) {
    let lists = [
        (
            actual.get_face_orientation().unwrap(),
            &expected.face.mean[..3],
        ),
        (
            actual.get_face_orientation_std().unwrap(),
            &expected.face.std[..3],
        ),
        (
            actual.get_face_position().unwrap(),
            &expected.face.mean[3..5],
        ),
        (
            actual.get_face_position_std().unwrap(),
            &expected.face.std[3..5],
        ),
    ];
    for (actual, expected) in lists {
        let actual: Vec<_> = actual.iter().map(f32::to_bits).collect();
        let expected: Vec<_> = expected.iter().map(|value| value.to_bits()).collect();
        assert_eq!(actual, expected);
    }
    let actual = [
        actual.get_face_prob(),
        actual.get_left_eye_prob(),
        actual.get_right_eye_prob(),
        actual.get_left_blink_prob(),
        actual.get_right_blink_prob(),
        actual.get_sunglasses_prob(),
        actual.get_phone_prob(),
        actual.get_sleep_prob(),
    ];
    assert_eq!(
        actual.map(f32::to_bits),
        expected.probabilities.map(f32::to_bits)
    );
}

#[test]
fn driver_fields_preserve_both_predictions_when_encoded() {
    // Given distinct left/right face descriptions and all eight probabilities.
    let prediction = prediction();

    // When a prediction is serialized into the canonical event.
    let bytes = encode(&prediction, timing(), &[0, 127, 255]);

    // Then decoding preserves source field mapping and exact list lengths.
    let reader = capnp::serialize::read_message(bytes.as_slice(), Default::default()).unwrap();
    let event = reader.get_root::<event::Reader>().unwrap();
    let event::DriverStateV2(driver) = event.which().unwrap() else {
        panic!("expected driverStateV2")
    };
    let driver = driver.unwrap();
    assert_data(driver.get_left_driver_data().unwrap(), &prediction.left);
    assert_data(driver.get_right_driver_data().unwrap(), &prediction.right);
    assert_eq!(
        driver.get_wheel_on_right_prob().to_bits(),
        0.75_f32.to_bits()
    );
}

#[test]
fn event_metadata_and_raw_bytes_are_preserved_when_encoded() {
    // Given explicit event timing and opaque binary model output.
    let timing = timing();
    let raw = [0, 127, 255, 0, 1];

    // When a prediction is serialized.
    let bytes = encode(&prediction(), timing, &raw);

    // Then metadata, validity, and raw output are unchanged.
    let reader = capnp::serialize::read_message(bytes.as_slice(), Default::default()).unwrap();
    let event = reader.get_root::<event::Reader>().unwrap();
    assert!(event.get_valid());
    assert_eq!(event.get_log_mono_time(), timing.log_mono_time);
    let event::DriverStateV2(driver) = event.which().unwrap() else {
        panic!("expected driverStateV2")
    };
    let driver = driver.unwrap();
    assert_eq!(driver.get_frame_id(), timing.frame_id);
    assert_eq!(
        driver.get_model_execution_time().to_bits(),
        timing.model_execution_time.to_bits()
    );
    assert_eq!(
        driver.get_gpu_execution_time().to_bits(),
        timing.gpu_execution_time.to_bits()
    );
    assert_eq!(driver.get_raw_predictions().unwrap(), raw);
}

#[test]
fn raw_predictions_remain_present_when_payload_is_empty() {
    // Given disabled raw-output publication.
    let raw = [];

    // When the empty payload is encoded.
    let bytes = encode(&prediction(), timing(), &raw);

    // Then the canonical rawPredictions field is initialized and empty.
    let reader = capnp::serialize::read_message(bytes.as_slice(), Default::default()).unwrap();
    let event = reader.get_root::<event::Reader>().unwrap();
    let event::DriverStateV2(driver) = event.which().unwrap() else {
        panic!("expected driverStateV2")
    };
    let driver = driver.unwrap();
    assert!(driver.has_raw_predictions());
    assert!(driver.get_raw_predictions().unwrap().is_empty());
}
