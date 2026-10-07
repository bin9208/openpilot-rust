use openpilot_control_policy::vehicle::Physical;
use openpilot_control_tools::joystickd::{Config, Controller, Input, LongState};

fn owner() -> Controller {
    Controller::new(Config {
        physical: Physical {
            mass: 1600.0,
            inertia: 2500.0,
            wheelbase: 2.5,
            center_front: 1.0,
            rear_ratio: 0.0,
            stiffness_front: 100_000.0,
            stiffness_rear: 100_000.0,
            steer_ratio: 16.0,
        },
        stopping_speed: 0.5,
        openpilot_longitudinal: true,
        pcm_cruise: true,
    })
}

#[test]
fn joystick_expires_after_twenty_frames_and_short_stale_axes_are_ignored() {
    let controller = owner();
    let mut input = Input {
        enabled: true,
        frame: 21,
        joystick_frame: 1,
        axes: vec![1.0],
        ..Input::default()
    };
    assert_eq!(controller.control(&input).unwrap().accel, 4.0);

    input.frame = 22;
    input.axes.clear();
    let result = controller.control(&input).unwrap();
    assert_eq!(result.accel, 0.0);
    assert_eq!(result.long_state, LongState::Stopping);
    assert!(!result.resume);
}

#[test]
fn inactive_control_does_not_index_missing_axes() {
    let controller = owner();
    let input = Input {
        enabled: true,
        active: true,
        steer_fault_permanent: true,
        override_longitudinal: true,
        frame: 2,
        joystick_frame: 1,
        ..Input::default()
    };
    let result = controller.control(&input).unwrap();
    assert!(!result.lat_active);
    assert!(!result.long_active);
    assert_eq!(result.long_state, LongState::Off);
}

#[test]
fn fresh_active_control_reports_a_missing_axis() {
    let controller = owner();
    let input = Input {
        active: true,
        frame: 2,
        joystick_frame: 1,
        axes: vec![0.0],
        ..Input::default()
    };
    assert!(controller.control(&input).is_err());
}

#[test]
fn source_stopping_boundary_and_cruise_resume_follow_acceleration() {
    let controller = owner();
    let input = Input {
        enabled: true,
        speed: 0.5,
        frame: 2,
        joystick_frame: 1,
        axes: vec![0.25],
        ..Input::default()
    };
    let result = controller.control(&input).unwrap();
    assert_eq!(result.accel, 1.0);
    assert!(result.resume);
    assert_eq!(result.long_state, LongState::Stopping);
}
