use openpilot_encoderd::{
    config::{Camera, Mode},
    lifecycle::{Frame, Lifecycle},
    sync::Synchronization,
};

#[test]
fn youtube_wide_selects_the_original_wide_stream() {
    assert_eq!(Mode::YoutubeWide.cameras(), &[Camera::WideRoad]);
}

#[test]
fn ready_frames_are_dropped_before_all_cameras_reach_the_start_id() {
    let mut state = Synchronization::new(3);
    assert!(!state.frame(Camera::Road, 101).encode);
    assert!(!state.frame(Camera::Driver, 103).encode);
    assert!(!state.frame(Camera::WideRoad, 105).encode);
    assert_eq!(state.start_frame_id, 107);
    assert!(!state.frame(Camera::Road, 106).encode);
    assert!(state.frame(Camera::Road, 107).encode);
    assert!(state.frame(Camera::Road, 0).encode);
}

#[test]
fn lag_drop_does_not_consume_carrot_prewarm_and_idle_resumes_without_rotation() {
    let mut state = Lifecycle::new(true, 60);
    let frame = Frame {
        buffer_frame_id: 100,
        frame_id: 100,
        synced: true,
        exit: false,
        session_active: false,
        start_frame_id: 100,
    };
    assert!(
        state
            .matching_frame(Frame {
                buffer_frame_id: 101,
                ..frame
            })
            .log_lag
    );
    assert!(
        !state
            .matching_frame(Frame {
                buffer_frame_id: 102,
                ..frame
            })
            .log_lag
    );
    let prewarm = state.matching_frame(frame);
    assert!(prewarm.encode && prewarm.thumbnail && !prewarm.rotate);
    state.encoded();
    assert_eq!(state.matching_frame(frame).idle, Some(true));
    assert!(
        state
            .matching_frame(Frame {
                session_active: true,
                frame_id: 2000,
                buffer_frame_id: 2000,
                ..frame
            })
            .encode
    );
    assert_eq!(state.segment, 0);
}

#[test]
fn skipped_segments_rotate_only_once_for_each_processed_frame() {
    let mut state = Lifecycle::new(false, 60);
    let frame = Frame {
        buffer_frame_id: 3700,
        frame_id: 3700,
        synced: true,
        exit: false,
        session_active: true,
        start_frame_id: 100,
    };
    for segment in 1..=3 {
        assert!(state.matching_frame(frame).rotate);
        state.rotated();
        assert_eq!(state.segment, segment);
    }
    assert!(!state.matching_frame(frame).rotate);
}
