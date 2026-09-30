use openpilot_modeld::{
    calibration::{CalibrationUpdate, DrivingCalibration},
    camera::{receive_pair, select_streams, CameraSource, CameraStream, Captured, FrameMeta},
    inputs::{DropTracker, PolicyInputs},
};
use std::{collections::VecDeque, convert::Infallible};

struct Camera {
    frames: VecDeque<Option<Captured<u32>>>,
    receives: usize,
}

impl Camera {
    fn new(values: &[(u32, u64)]) -> Self {
        Self {
            frames: values
                .iter()
                .map(|&(frame_id, timestamp_sof)| {
                    Some(Captured {
                        metadata: FrameMeta {
                            frame_id,
                            timestamp_sof,
                            timestamp_eof: timestamp_sof + 1000,
                        },
                        buffer: frame_id,
                    })
                })
                .collect(),
            receives: 0,
        }
    }
}

impl CameraSource for Camera {
    type Buffer = u32;
    type Error = Infallible;

    fn receive(&mut self) -> Result<Option<Captured<u32>>, Infallible> {
        self.receives += 1;
        Ok(self.frames.pop_front().flatten())
    }
}

#[test]
fn pairs_only_current_exposures_and_advances_the_older_camera() {
    let mut main = Camera::new(&[(10, 100_000_000), (11, 150_000_000)]);
    let mut extra = Camera::new(&[(20, 50_000_000), (21, 150_000_000)]);
    let pair = receive_pair(&mut main, Some(&mut extra)).unwrap().unwrap();
    assert_eq!(pair.main().metadata.frame_id, 11);
    assert_eq!(pair.extra().metadata.frame_id, 21);
    assert_eq!((main.receives, extra.receives), (2, 2));
    assert!(receive_pair(&mut main, Some(&mut extra)).unwrap().is_none());
    assert_eq!(extra.receives, 2);
}

#[test]
fn skew_boundary_and_resynchronization_count_match_source() {
    for (skew, accepted) in [(20_000_000, true), (20_000_001, false)] {
        let mut main = Camera::new(&[(1, 100_000_000)]);
        let mut extra = Camera::new(&[(1, 100_000_000 + skew)]);
        assert_eq!(
            receive_pair(&mut main, Some(&mut extra)).unwrap().is_some(),
            accepted
        );
    }
    let mut main = Camera::new(
        &(0..100)
            .map(|i| (i, u64::from(i) * 100_000_000))
            .collect::<Vec<_>>(),
    );
    let mut extra = Camera::new(
        &(0..100)
            .map(|i| (i, u64::from(i) * 100_000_000 + 50_000_000))
            .collect::<Vec<_>>(),
    );
    assert!(receive_pair(&mut main, Some(&mut extra)).unwrap().is_none());
    assert_eq!(main.receives + extra.receives, 12);
}

#[test]
fn single_camera_reuses_the_same_buffer_and_stream_selection_honors_wide_toggle() {
    let mut main = Camera::new(&[(0, 10)]);
    let pair = receive_pair(&mut main, None).unwrap().unwrap();
    assert!(std::ptr::eq(pair.main(), pair.extra()));
    assert_eq!(
        select_streams(true, true, false),
        Some((CameraStream::Road, false))
    );
    assert_eq!(
        select_streams(true, true, true),
        Some((CameraStream::Road, true))
    );
    assert_eq!(
        select_streams(false, true, true),
        Some((CameraStream::WideRoad, false))
    );
    assert_eq!(select_streams(false, true, false), None);
}

#[test]
fn drop_warmup_does_not_hide_prepare_only_or_counter_resets() {
    let mut tracker = DropTracker::default();
    let first = tracker.observe(200);
    assert_eq!(first.dropped, 199);
    assert!(first.prepare_only);
    assert_eq!(first.ratio, 0.0);
    for id in 201..210 {
        assert_eq!(tracker.observe(id).ratio, 0.0);
    }
    let gap = tracker.observe(212);
    assert_eq!(gap.dropped, 2);
    let filtered = (0.05 / 10.05) * 2.0;
    assert_eq!(gap.ratio, filtered / (1.0 + filtered));
    let reset = tracker.observe(0);
    assert_eq!(reset.dropped, 0);
    assert!(!reset.prepare_only);
    tracker.reset_warmup();
    assert_eq!(tracker.observe(2).ratio, 0.0);
}

#[test]
fn packed_inputs_preserve_pulses_even_when_policy_is_skipped() {
    let mut inputs = PolicyInputs::new(3).unwrap();
    inputs.update(3, false, 0.275, 0.375);
    assert_eq!(
        &inputs.packed()[..8],
        &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(&inputs.packed()[8..12], &[1.0, 0.0, 0.275, 0.375]);
    inputs.update(3, true, 0.1, 0.2);
    assert_eq!(&inputs.packed()[..8], &[0.0; 8]);
    assert_eq!(&inputs.packed()[8..10], &[0.0, 1.0]);
    inputs.update(0, false, 0.0, 0.0);
    inputs.update(3, false, 0.0, 0.0);
    assert_eq!(inputs.packed()[3], 1.0);
    inputs.set_features(&[1.0, 2.0, 3.0]).unwrap();
    assert!(inputs.set_features(&[4.0, 5.0]).is_err());
    inputs.update(9, false, 0.0, 0.0);
    assert_eq!(&inputs.packed()[12..], &[1.0, 2.0, 3.0]);
    assert_eq!(&inputs.packed()[..8], &[0.0; 8]);
}

#[test]
fn calibration_requires_update_and_camera_metadata_but_not_calibrated_status() {
    let mut state = DrivingCalibration::default();
    let mut input = CalibrationUpdate {
        updated: true,
        road_seen: false,
        device_seen: true,
        rpy: [0.0; 3],
        calibrated: false,
        yaw_trim_degrees: 2.0,
        device: "tici",
        sensor: "os04c10",
        main_wide: false,
        use_extra: true,
    };
    assert!(!state.update(input).unwrap());
    assert!(!state.seen());
    assert_eq!(state.main(), [0.0; 9]);
    input.road_seen = true;
    assert!(state.update(input).unwrap());
    assert!(state.seen());
    let before = state.main();
    input.calibrated = true;
    input.updated = false;
    assert!(!state.update(input).unwrap());
    assert_eq!(state.main(), before);
    input.updated = true;
    assert!(state.update(input).unwrap());
    assert_ne!(state.main(), before);
    let before = state.main();
    input.sensor = "invalid";
    assert!(state.update(input).is_err());
    assert_eq!(state.main(), before);
}
