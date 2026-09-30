use openpilot_driving_modeld::state::{Settings, State};
use openpilot_modeld::action::Action;

#[test]
fn loop_refresh_counts_camera_timeouts_and_action_feedback_is_float32() {
    let mut state = State::new(0.2, 0.5, 0.1);
    assert_eq!(
        state.action_times(),
        (0.07500000000000001, 0.5750000000000001)
    );
    for _ in 0..99 {
        assert!(!state.begin_iteration());
    }
    assert!(state.begin_iteration());
    state.refresh(Settings {
        custom_lateral_delay: 0.2,
        lateral_smooth: 0.9,
        longitudinal_delay: 0.8,
        v_ego_stopping: 0.4,
        camera_yaw_trim: -0.2,
    });
    state.complete_action(
        Action {
            desired_curvature: 0.123456789,
            ..Action::default()
        },
        0.15,
    );
    assert_eq!(
        state.previous_action.desired_curvature,
        f64::from(0.123_456_79_f32)
    );
    assert_eq!(state.lateral_delay, 0.8);
    assert_eq!(
        state.action_times(),
        (0.8750000000000001, 0.8750000000000001)
    );
    state.settings.custom_lateral_delay = 0.0;
    state.settings.lateral_smooth = -1.0;
    state.complete_action(Action::default(), 0.15);
    assert_eq!(state.lateral_delay, 0.15);
}
