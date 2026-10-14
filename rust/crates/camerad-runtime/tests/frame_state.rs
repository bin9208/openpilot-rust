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

#[test]
fn sparse_ae_is_bit_identical_for_every_sensor_camera_and_exposure_update() {
    use openpilot_camerad::{geometry::luminance_samples, nv12::Nv12Layout};
    for sensor in [SensorKind::Ar0231, SensorKind::Ox03c10, SensorKind::Os04c10] {
        let config = sensor.config();
        let scale = config.out_scale as u32;
        let height = if config.hdr_offset > 0 {
            (config.frame_height - config.hdr_offset as u32) / 2
        } else {
            config.frame_height
        } / scale;
        let width = config.frame_width / scale;
        let layout = Nv12Layout::new(width, height).unwrap();
        for (camera, focal) in [
            (CameraId::Road, 8.0),
            (CameraId::Wide, 1.71),
            (CameraId::Driver, 1.71),
        ] {
            let geometry = Geometry {
                width: width as i32,
                height: height as i32,
                focal_mm: focal,
            };
            let mut whole = FrameState::new(sensor, camera, geometry).unwrap();
            let mut compact = FrameState::new(sensor, camera, geometry).unwrap();
            let mut pixels = vec![0; layout.size as usize];
            let plan = compact.sample_indices(pixels.len()).unwrap();
            let mut samples = vec![0; plan.len()];
            assert_eq!(pixels.capacity(), layout.size as usize);
            assert_eq!(samples.capacity(), plan.len());
            assert!(
                samples.len() < pixels.len() / 4,
                "sample storage must replace the full NV12 allocation"
            );
            println!(
                "{sensor:?}/{camera:?} {}x{} stride={} full={} sample={}",
                width,
                height,
                layout.stride,
                pixels.len(),
                samples.len()
            );
            for frame_id in 0..9 {
                for (offset, pixel) in pixels.iter_mut().enumerate() {
                    *pixel = ((offset * 37 + frame_id as usize * 79) % 256) as u8;
                }
                for (byte, offset) in samples.iter_mut().zip(plan.clone()) {
                    *byte = pixels[offset];
                }
                let old = openpilot_camerad::geometry::luminance(
                    &pixels,
                    whole.region(),
                    openpilot_camerad::geometry::Sampling {
                        width: width as usize,
                        x_skip: 2,
                        y_skip: if matches!(camera, CameraId::Driver) {
                            4
                        } else {
                            2
                        },
                    },
                )
                .unwrap();
                assert_eq!(old.to_bits(), luminance_samples(&samples).to_bits());
                let enabled = frame_id != 4;
                let manual = if frame_id >= 5 {
                    ("1", "123")
                } else {
                    ("", "")
                };
                let registers = whole
                    .adjust_with_manual(frame_id, &pixels, enabled, || Ok(manual))
                    .unwrap();
                let sampled = compact
                    .adjust_samples_with_manual(frame_id, &samples, enabled, || Ok(manual))
                    .unwrap();
                assert_eq!(
                    registers.as_ref().map(|value| value.as_slice()),
                    sampled.as_ref().map(|value| value.as_slice())
                );
                assert_eq!(
                    serde_json::to_value(whole.exposure()).unwrap(),
                    serde_json::to_value(compact.exposure()).unwrap()
                );
                let frame = FrameMetadata {
                    slot: 0,
                    frame_id,
                    request_id: frame_id,
                    timestamp_sof: 10,
                    timestamp_eof: 20,
                    processing_time: 0.01,
                };
                assert_eq!(
                    whole.encode(frame, 30, false, None).unwrap(),
                    compact.encode(frame, 30, false, None).unwrap()
                );
            }
        }
    }
}

#[test]
fn truncated_compact_input_fails_before_manual_read_or_exposure_mutation() {
    let mut state = FrameState::new(
        SensorKind::Os04c10,
        CameraId::Road,
        Geometry {
            width: 1344,
            height: 760,
            focal_mm: 8.0,
        },
    )
    .unwrap();
    let before = serde_json::to_value(state.exposure()).unwrap();
    assert!(state
        .adjust_samples_with_manual::<&str, &str>(1, &[], true, || panic!(
            "bad sample input must not read Params"
        ))
        .is_err());
    assert_eq!(before, serde_json::to_value(state.exposure()).unwrap());
}
