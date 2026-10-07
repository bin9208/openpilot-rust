use openpilot_control_tools::longitudinal_maneuvers::{Controller, Input};

#[test]
fn ready_requires_more_than_sixty_model_updates() {
    let mut owner = Controller::new(0.5).unwrap();
    let input = Input {
        speed: 5.0,
        active: true,
        ..Input::default()
    };
    for _ in 0..60 {
        let result = owner.step(&input).unwrap();
        assert_eq!(result.acceleration, 0.0);
        assert!(!result.state.unwrap().active);
    }
    let result = owner.step(&input).unwrap();
    assert_eq!(result.acceleration, -0.5);
    assert!(result.state.unwrap().active);
}

#[test]
fn active_run_keeps_source_acceleration_when_long_active_falls() {
    let mut owner = Controller::new(0.5).unwrap();
    let input = Input {
        speed: 5.0,
        active: true,
        ..Input::default()
    };
    for _ in 0..61 {
        owner.step(&input).unwrap();
    }
    let result = owner
        .step(&Input {
            speed: 0.5,
            cruise_standstill: true,
            ..Input::default()
        })
        .unwrap();
    assert_eq!(result.acceleration, -0.5);
    assert!(!result.should_stop);
    assert_eq!(result.state.unwrap().ready_count, 0);
    let result = owner
        .step(&Input {
            speed: -1.0,
            ..Input::default()
        })
        .unwrap();
    assert!(result.should_stop);
}

#[test]
fn completed_preset_selects_next_on_following_update() {
    let mut owner = Controller::new(0.5).unwrap();
    let input = Input {
        speed: 5.0,
        active: true,
        ..Input::default()
    };
    let mut completed = None;
    for _ in 0..1000 {
        let result = owner.step(&input).unwrap();
        if result.state.as_ref().is_some_and(|state| state.finished) {
            completed = Some(result);
            break;
        }
    }
    let completed = completed.unwrap();
    assert_eq!(completed.selected, Some(0));
    assert_eq!(completed.state.unwrap().repeated, 2);
    let next = owner
        .step(&Input {
            standstill: true,
            ..Input::default()
        })
        .unwrap();
    assert_eq!(next.selected, Some(1));
    assert!(next.should_stop);
    assert_eq!(next.alert_text2, "start from stop");
}
