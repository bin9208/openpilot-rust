use openpilot_monitoring::{AlertLevel, DriverMonitoring, Input, Policy};

#[test]
fn sleep_alone_requires_disengagement_after_red() {
    for rhd in [false, true] {
        let mut dm = DriverMonitoring::new(rhd, false, false);
        let mut input = Input::default();
        input.driver.left.sleep_prob = 0.9;
        input.driver.right.sleep_prob = 0.9;
        for _ in 0..280 {
            dm.run_step(&input).unwrap();
        }
        assert_eq!(dm.alert_level, AlertLevel::Three);
        assert!(dm.distracted_types.sleep);
        assert!(!dm.distracted_types.eye);
        input.driver.left.sleep_prob = 0.;
        input.driver.right.sleep_prob = 0.;
        for _ in 0..200 {
            dm.run_step(&input).unwrap();
        }
        assert_eq!(dm.alert_level, AlertLevel::Three);
        assert!(dm.too_distracted);
        input.enabled = false;
        dm.run_step(&input).unwrap();
        assert_eq!(dm.alert_level, AlertLevel::None);
    }
}

#[test]
fn orange_cannot_escape_by_hiding_face_or_touching_wheel() {
    let mut dm = DriverMonitoring::new(false, false, false);
    let mut input = Input::default();
    input.driver.left.phone_prob = 1.;
    for _ in 0..180 {
        dm.run_step(&input).unwrap();
    }
    assert_eq!(dm.alert_level, AlertLevel::Two);
    input.driver.left.face_prob = 0.;
    input.steering_pressed = true;
    let awareness = dm.awareness;
    for _ in 0..200 {
        dm.run_step(&input).unwrap();
    }
    assert_eq!(dm.active_policy, Policy::Vision);
    assert_eq!(dm.alert_level, AlertLevel::Two);
    assert_eq!(dm.awareness, awareness);
}

#[test]
fn uncertain_model_falls_back_on_frame_after_ten_seconds() {
    let mut dm = DriverMonitoring::new(false, false, false);
    let mut input = Input::default();
    input.driver.left.face_orientation_std = Some(vec![0.4, 0.4, 0.4]);
    for _ in 0..200 {
        dm.run_step(&input).unwrap();
    }
    assert_eq!(dm.active_policy, Policy::Vision);
    dm.run_step(&input).unwrap();
    assert_eq!(dm.active_policy, Policy::Wheeltouch);
}

#[test]
fn saved_lockout_expires_only_after_thirty_minutes() {
    let mut dm = DriverMonitoring::new(false, false, true);
    let input = Input {
        enabled: false,
        ..Input::default()
    };
    for _ in 0..36000 {
        dm.run_step(&input).unwrap();
    }
    assert!(dm.too_distracted);
    dm.run_step(&input).unwrap();
    assert!(!dm.too_distracted);
    assert_eq!(dm.lockout_time, 0);
}

#[test]
fn low_speed_and_always_on_preserve_alert_limits() {
    for always_on in [false, true] {
        let mut dm = DriverMonitoring::new(false, always_on, false);
        let mut input = Input {
            enabled: !always_on,
            car_speed: if always_on { 20. } else { 0. },
            ..Input::default()
        };
        input.driver.left.sleep_prob = 1.;
        for _ in 0..1000 {
            dm.run_step(&input).unwrap();
        }
        assert_eq!(
            dm.alert_level,
            if always_on {
                AlertLevel::Two
            } else {
                AlertLevel::None
            }
        );
        assert_eq!(dm.alert_3_cnt, 0);
    }
}

#[test]
fn malformed_percent_and_count_fail_instead_of_saturating() {
    let mut dm = DriverMonitoring::new(false, false, false);
    dm.awareness = f64::NAN;
    assert!(dm.state_packet(true, 0).is_err());
    dm.awareness = 1.;
    dm.alert_3_cnt = 128;
    assert!(dm.state_packet(true, 0).is_err());
}

#[test]
fn nan_uncertainty_does_not_become_a_confident_face() {
    let mut dm = DriverMonitoring::new(false, false, false);
    let mut input = Input::default();
    input.driver.left.face_orientation_std = Some(vec![f64::NAN, 0., 0.]);
    dm.run_step(&input).unwrap();
    assert!(dm.model_std_max.is_nan());
    assert!(!dm.pose.low_std);
    assert_eq!(dm.hi_stds, 1);
}
