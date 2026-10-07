use openpilot_control_tools::lateral_maneuvers::{Controller, Input};

fn ready() -> Input {
    Input {
        speed: 20.0 * (1.609344 * (1.0 / 3.6)),
        active: true,
        curvature: 0.001,
        orientation: vec![0.0; 3],
        ..Input::default()
    }
}

#[test]
fn readiness_captures_baseline_and_ignores_wire_validity() {
    let mut owner = Controller::new().unwrap();
    let mut input = ready();
    for _ in 0..40 {
        assert!(!owner.step(&input).unwrap().valid);
    }
    input.curvature = -0.001;
    let command = owner.step(&input).unwrap();
    assert!(command.valid);
    assert_eq!(command.baseline, input.curvature);
    assert_eq!(command.acceleration, 0.5);
    input.active = false;
    input.orientation = vec![0.1; 3];
    assert!(owner.step(&input).unwrap().valid);
    input.steering_pressed = true;
    let reset = owner.step(&input).unwrap();
    assert!(!reset.valid);
    assert_eq!(reset.state.unwrap().ready_count, 0);
    input = ready();
    for _ in 0..41 {
        owner.step(&input).unwrap();
    }
    input.speed += 0.8;
    assert!(!owner.step(&input).unwrap().valid);
}

#[test]
fn holdoff_preserves_completed_state_and_final_baseline_tick() {
    let mut owner = Controller::new().unwrap();
    let input = ready();
    let mut final_run = None;
    for _ in 0..1000 {
        let command = owner.step(&input).unwrap();
        if command.state.as_ref().is_some_and(|state| state.finished) {
            final_run = Some(command);
            break;
        }
    }
    let completed = final_run.unwrap();
    assert!(!completed.valid);
    assert_eq!(completed.complete_remaining, 20);
    let invalid = Input {
        speed: -1.0,
        steering_pressed: true,
        ..Input::default()
    };
    for remaining in (1..20).rev() {
        let command = owner.step(&invalid).unwrap();
        assert_eq!(command.alert_text1, "Completed");
        assert_eq!(command.complete_remaining, remaining);
        assert_eq!(
            command.state.as_ref().unwrap().action_frames,
            completed.state.as_ref().unwrap().action_frames
        );
        assert!(!command.valid);
    }
    let last = owner.step(&invalid).unwrap();
    assert!(last.valid);
    assert_eq!(last.curvature, input.curvature);
    assert_eq!(last.acceleration, 0.0);
    assert_eq!(last.selected, Some(0));
    assert_eq!(owner.step(&input).unwrap().selected, Some(1));
}

#[test]
fn setup_holdoff_only_holds_first_alert_line() {
    let mut owner = Controller::new().unwrap();
    let first = owner.step(&Input::default()).unwrap();
    assert_eq!(first.alert_text1, "Set speed to 20 mph");
    assert_eq!(first.alert_text2, None);
    let second = owner.step(&ready()).unwrap();
    assert_eq!(second.alert_text1, first.alert_text1);
    assert_eq!(second.alert_text2, Some("step right 20mph"));
    assert_eq!(second.display_holdoff, 9);
}
