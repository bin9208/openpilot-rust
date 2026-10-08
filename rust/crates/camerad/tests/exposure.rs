use openpilot_camerad::exposure::{CameraId, ExposureState, FrameMeasurement, ManualExposure};
use openpilot_camerad::sensor::SensorKind;

#[test]
fn exposure_uses_three_frame_history_and_disabled_frames_do_not_change_it() {
    let mut state = ExposureState::new(SensorKind::Ar0231, CameraId::Road);
    assert_eq!(state.cur_ev.map(f32::to_bits), [4.0_f32.to_bits(); 3]);
    let before = state.clone();
    assert!(state
        .update(
            FrameMeasurement {
                frame_id: 1,
                grey: 0.0,
                enabled: false
            },
            ManualExposure::default()
        )
        .expect("disabled frame")
        .is_none());
    assert_eq!(
        state.cur_ev.map(f32::to_bits),
        before.cur_ev.map(f32::to_bits)
    );
    assert!(state
        .update(
            FrameMeasurement {
                frame_id: 1,
                grey: 0.0,
                enabled: true
            },
            ManualExposure::default()
        )
        .expect("black frame")
        .is_some());
    assert_eq!(state.cur_ev[0].to_bits(), before.cur_ev[0].to_bits());
    assert_eq!(state.cur_ev[2].to_bits(), before.cur_ev[2].to_bits());
    assert_ne!(state.cur_ev[1].to_bits(), before.cur_ev[1].to_bits());
    assert_eq!(state.exposure_time, 2133);
}

#[test]
fn three_frame_sum_preserves_the_source_fused_operand_order() {
    let mut state = ExposureState::new(SensorKind::Ar0231, CameraId::Wide);
    for frame_id in 1..=16 {
        state
            .update(
                FrameMeasurement {
                    frame_id,
                    grey: 0.0,
                    enabled: (frame_id - 1) % 7 != 0,
                },
                ManualExposure::default(),
            )
            .expect("source black-frame trace");
    }
    assert_eq!(state.best_ev_score.to_bits(), 51.660_156_f32.to_bits());
    assert_eq!(state.exposure_time, 2064);
    assert_eq!(state.gain_idx, 13);
}
