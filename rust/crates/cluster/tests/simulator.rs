use openpilot_cluster::models::SimulatorInput;
use openpilot_cluster::simulator::gamepad::{DualSenseState, GamepadSample};
use openpilot_cluster::simulator::{ClusterSimulator, RandomInputSource};

#[test]
fn simulator_signal_moves_then_recenters_without_losing_motion() {
    let mut simulator = ClusterSimulator::default();
    let command = SimulatorInput {
        throttle: 0.5,
        left_signal_requested: true,
        ..SimulatorInput::default()
    };
    let first = simulator.update(&command, 0.1);
    assert_eq!(first.lane_change.as_deref(), Some("left"));
    assert_eq!(first.lane_change_phase, "changing");
    assert!(first.speed_kph > 0.0 && first.ego_lane_offset < 0.0);
    let command = SimulatorInput {
        throttle: 0.5,
        ..SimulatorInput::default()
    };
    let mut last = first;
    for _ in 0..100 {
        last = simulator.update(&command, 0.1);
    }
    assert_eq!(last.lane_change_phase, "idle");
    assert!(last.lane_change.is_none());
    assert_eq!(last.road_view_lane_position.to_bits(), (-1.0_f64).to_bits());
    assert!(!last.left_signal);
}

#[test]
fn random_input_alternates_signals_without_catching_up_multiple_expired_periods() {
    let mut source = RandomInputSource::default();
    assert!(!source.update(3.9).left_signal_requested);
    assert!(source.update(0.1).left_signal_requested);
    let next = source.update(100.0);
    assert!(!next.left_signal_requested && next.right_signal_requested);
    let next = source.update(0.0);
    assert!(!next.left_signal_requested && !next.right_signal_requested);
}

#[test]
fn gamepad_warmup_and_sdl_controller_joystick_scales_remain_distinct() {
    let mut state = DualSenseState::new(10.0);
    let mut buttons = vec![false; 15];
    buttons[9] = true;
    let controller = GamepadSample::Controller {
        axes: vec![32767, 0, 0, 0, 0, 32767],
        buttons: buttons.clone(),
    };
    assert_eq!(
        state.read_sample(&controller, 10.59).throttle.to_bits(),
        0.0_f64.to_bits()
    );
    let live = state.read_sample(&controller, 10.6);
    assert_eq!(live.throttle.to_bits(), 1.0_f64.to_bits());
    assert!(live.left_signal_requested);
    let joystick = GamepadSample::Joystick {
        axes: vec![32767, 0, 0, 0, 0, 32767],
        buttons,
        hat_x: 1,
    };
    let generic = state.read_sample(&joystick, 10.61);
    assert_eq!(
        generic.throttle.to_bits(),
        (32767.0_f64 / 32768.0).to_bits()
    );
    assert!(generic.left_signal_requested && generic.right_signal_requested);
}
