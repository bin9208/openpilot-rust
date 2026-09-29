use openpilot_cereal::log_capnp::model_data_v2::ConfidenceClass;
use openpilot_modeld::publication::PublishState;

#[test]
fn fcw_requires_full_history_and_strict_thresholds() {
    let mut state = PublishState::default();
    let mut meta = [0.0; 55];
    meta[4] = 0.8;
    meta[6] = 0.2;
    for frame in 1..5 {
        assert!(!state.update(&meta, frame).hard_brake);
    }
    assert!(state.update(&meta, 5).hard_brake);
    meta[6] = 0.15;
    assert!(!state.update(&meta, 6).hard_brake);
}

#[test]
fn confidence_only_updates_every_forty_frames_and_preserves_nan_red() {
    let mut state = PublishState::default();
    let mut meta = [0.0; 55];
    meta[1] = 1.0;
    meta[7] = 1.0;
    assert_eq!(state.update(&meta, 39).confidence, ConfidenceClass::Green);
    assert_eq!(state.update(&meta, 40).confidence, ConfidenceClass::Red);
}
