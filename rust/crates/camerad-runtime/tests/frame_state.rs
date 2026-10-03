use openpilot_camerad::{
    exposure::{CameraId, ManualExposure},
    geometry::Geometry,
    requests::FrameMetadata,
    sensor::SensorKind,
};
use openpilot_camerad_runtime::FrameState;

#[test]
fn published_exposure_is_the_previous_command_and_raw_is_road_decimated() {
    let mut state = FrameState::new(
        SensorKind::Os04c10,
        CameraId::Road,
        Geometry {
            width: 1928,
            height: 1208,
            focal_mm: 8.0,
        },
    )
    .unwrap();
    let frame = FrameMetadata {
        slot: 0,
        frame_id: 5,
        request_id: 23,
        timestamp_sof: 1_000_000_000,
        timestamp_eof: 1_011_000_000,
        processing_time: 0.01,
    };
    let before = state
        .encode(frame, 1_021_000_000, true, Some(&[1, 2, 3, 4]))
        .unwrap();
    let reader = capnp::serialize::read_message(
        std::io::Cursor::new(before),
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    let event = reader
        .get_root::<openpilot_cereal::log_capnp::event::Reader>()
        .unwrap();
    assert!(event.get_valid());
    assert_eq!(event.get_log_mono_time(), 1_021_000_000);
    let openpilot_cereal::log_capnp::event::Which::RoadCameraState(data) = event.which().unwrap()
    else {
        panic!("wrong camera service")
    };
    let data = data.unwrap();
    assert_eq!(data.get_frame_id(), 5);
    assert_eq!(data.get_request_id(), 23);
    assert_eq!(data.get_integ_lines(), 5);
    assert_eq!(data.get_gain(), 0.0);
    assert_eq!(data.get_target_grey_fraction(), 0.125);
    assert_eq!(data.get_image().unwrap(), &[1, 2, 3, 4]);
    assert!(state.wants_raw(frame.frame_id, true));
    assert!(!state.wants_raw(6, true));
    assert!(!state.wants_raw(5, false));
    let pixels = vec![32; 1928 * 1208];
    let writes = state
        .adjust(
            5,
            &pixels,
            true,
            ManualExposure {
                gain: "0",
                time: "2309",
            },
        )
        .unwrap()
        .unwrap();
    assert!(writes
        .as_slice()
        .iter()
        .any(|register| register.0 == 0x3502 && register.1 == 5));
    assert_eq!(state.exposure().exposure_time, 2309);
    assert_eq!(data.get_integ_lines(), 5);
}

#[test]
fn disabled_sensor_keeps_exposure_and_missing_requested_raw_is_fatal() {
    let mut state = FrameState::new(
        SensorKind::Os04c10,
        CameraId::Road,
        Geometry {
            width: 1928,
            height: 1208,
            focal_mm: 8.0,
        },
    )
    .unwrap();
    let pixels = vec![0; 1928 * 1208];
    assert!(state
        .adjust(
            2,
            &pixels,
            false,
            ManualExposure {
                gain: "bad",
                time: "bad"
            }
        )
        .unwrap()
        .is_none());
    assert_eq!(state.exposure().exposure_time, 5);
    let frame = FrameMetadata {
        slot: 0,
        frame_id: 5,
        request_id: 0,
        timestamp_sof: 0,
        timestamp_eof: 0,
        processing_time: 0.0,
    };
    assert!(state.encode(frame, 0, true, None).is_err());
}

#[test]
fn manual_params_are_lazy_and_failure_keeps_the_source_partial_update() {
    let mut state = FrameState::new(
        SensorKind::Os04c10,
        CameraId::Road,
        Geometry {
            width: 1928,
            height: 1208,
            focal_mm: 8.0,
        },
    )
    .unwrap();
    let pixels = vec![32; 1928 * 1208];
    let disabled = state
        .adjust_with_manual(
            0,
            &pixels,
            false,
            || -> Result<(&str, &str), openpilot_camerad::exposure::ExposureError> {
                panic!("disabled camera must not open Params")
            },
        )
        .unwrap();
    assert!(disabled.is_none());
    let failure = state
        .adjust_with_manual(
            0,
            &pixels,
            true,
            || -> Result<(&str, &str), openpilot_camerad::exposure::ExposureError> {
                Err(openpilot_camerad::exposure::ExposureError::ManualInput(
                    Box::new(std::io::Error::other("Params unavailable")),
                ))
            },
        )
        .unwrap_err();
    assert!(matches!(
        failure,
        openpilot_camerad_runtime::FrameStateError::Exposure(
            openpilot_camerad::exposure::ExposureError::ManualInput(_)
        )
    ));
    assert_eq!(state.exposure().best_ev_score, 1e6);
    assert_eq!(
        (state.exposure().new_exp_g, state.exposure().new_exp_t),
        (0, 0)
    );
    assert_eq!(state.exposure().exposure_time, 5);
    assert_eq!(state.exposure().measured_grey_fraction, 0.0);
}
