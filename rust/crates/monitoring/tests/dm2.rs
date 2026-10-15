use openpilot_monitoring::{AlertLevel, DriverMonitoring2, Input};
#[test]
fn unavailable_camera_uses_interaction_budget_and_keeps_terminal_lockout() {
    let mut dm = DriverMonitoring2::new(false, false, false);
    dm.configure_context(0., false, false, false);
    for i in 0..905 {
        dm.configure_context(i as f64 * 0.05, false, false, false);
        dm.run_without_camera(false, true, false, false);
    }
    assert_eq!(dm.monitor.alert_level, AlertLevel::Three);
    dm.configure_context(45.3, false, false, false);
    dm.record_interaction(45.3);
    assert!(dm.monitor.awareness <= 0.);
}
#[test]
fn mode_change_preserves_elapsed_attention_and_grace_is_edge_based() {
    let mut dm = DriverMonitoring2::new(false, false, true);
    dm.monitor.awareness = 0.5;
    dm.configure_context(10., true, false, false);
    assert!((dm.monitor.awareness - 0.75).abs() < 1e-10);
    dm.record_interaction(10.);
    assert_eq!(dm.interaction_grace_remaining(), 45.);
    dm.configure_context(11., true, true, false);
    assert_eq!(dm.interaction_grace_remaining(), 0.);
    dm.set_experimental(false);
    assert!(dm.monitor.awareness >= 0.);
    let _ = Input::default();
}
#[test]
fn parking_requires_continuous_full_second_and_does_not_reset_on_engage() {
    let mut dm = DriverMonitoring2::new(false, false, false);
    dm.monitor.too_distracted = true;
    for i in 0..20 {
        assert!(!dm.update_parked_reset(i as f64 * 0.05, true));
    }
    assert!(dm.update_parked_reset(1., true));
    assert!(!dm.monitor.too_distracted);
}
#[test]
fn context_and_edges_reject_held_and_automatic_button_credit() {
    use openpilot_monitoring::context::*;
    let mut edges = InteractionEdges::default();
    assert!(edges.update(0., true, false, false, &[]));
    assert!(!edges.update(0.05, true, false, false, &[]));
    let mut filter = AutomaticCancelFilter::default();
    filter.record(1., true);
    assert!(filter
        .filter_buttons(1.1, vec![("cancel".into(), true)])
        .is_empty());
    assert!(filter
        .filter_buttons(1.2, vec![("cancel".into(), false)])
        .is_empty());
    let mut camera = CameraAvailability::default();
    assert!(!camera.update(0., true));
    assert!(camera.update(2., true));
    assert!(!camera.update(2.1, false));
    assert!(!camera.update(3., true));
    let mut traffic = TrafficContext::default();
    let obj = ObjectObservation {
        x: 30.,
        y: 0.,
        speed: 20.,
        relative_speed: 0.,
    };
    assert_eq!(traffic.update(0., &[obj], true, true, true), (false, false));
    assert_eq!(traffic.update(0.2, &[obj], true, true, true), (true, false));
    assert_eq!(traffic.strict_until, 20.2);
    traffic.update(1., &[obj], true, true, true);
    assert_eq!(traffic.strict_until, 20.2);
}
#[test]
fn final_upstream_policy_trace_matches_at_every_checkpoint() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/dm2-final.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let camera = case["camera"].as_bool().unwrap();
        let experimental = case["experimental"].as_bool().unwrap();
        let mut dm = DriverMonitoring2::new(false, false, experimental);
        let mut input = Input::default();
        input.driver.left.phone_prob = 1.;
        input.driver.right.phone_prob = 1.;
        let trace = case["trace"].as_array().unwrap();
        let mut checkpoint = 0;
        for frame in 0..1100 {
            let now = frame as f64 * 0.05;
            dm.configure_context(
                now,
                camera,
                (400..600).contains(&frame),
                (200..400).contains(&frame),
            );
            if [100, 300, 700].contains(&frame) {
                dm.record_interaction(now);
            }
            if camera {
                dm.run_step(&input).unwrap();
            } else {
                dm.run_without_camera(false, true, false, false);
            }
            if checkpoint < trace.len() && trace[checkpoint]["frame"].as_u64() == Some(frame) {
                let expected = &trace[checkpoint];
                for (key, value) in [
                    ("awareness", dm.monitor.awareness),
                    ("wheel_factor", dm.wheel_factor),
                    ("vision_factor", dm.vision_factor),
                    ("grace", dm.interaction_grace_remaining()),
                    ("credit", dm.input_credit_seconds),
                ] {
                    assert!(
                        (value - expected[key].as_f64().unwrap()).abs() < 1e-10,
                        "{key} frame {frame} camera {camera} experimental {experimental}: actual {value}, expected {}",
                        expected[key]
                    );
                }
                let alert = match dm.monitor.alert_level {
                    AlertLevel::None => "0",
                    AlertLevel::One => "1",
                    AlertLevel::Two => "2",
                    AlertLevel::Three => "3",
                };
                assert_eq!(alert, expected["alert"].as_str().unwrap(), "frame {frame}");
                assert_eq!(
                    dm.monitor.alert_3_cnt as u64,
                    expected["count"].as_u64().unwrap()
                );
                assert_eq!(
                    dm.monitor.no_response_cnt as u64,
                    expected["no_response"].as_u64().unwrap()
                );
                assert_eq!(
                    dm.monitor.too_distracted,
                    expected["lockout"].as_bool().unwrap()
                );
                checkpoint += 1;
            }
        }
    }
}
#[test]
fn forward_attention_recovers_after_two_seconds_without_interaction_grace() {
    let mut dm = DriverMonitoring2::new(false, false, true);
    dm.configure_context(0., true, false, false);
    dm.monitor.awareness = 0.1;
    dm.monitor.alert_level = AlertLevel::Two;
    for i in 0..40 {
        dm.configure_context(i as f64 * 0.05, true, false, false);
        dm.run_step(&Input::default()).unwrap();
        if i < 39 {
            assert!(!dm.forward_recovery);
        }
    }
    assert!(dm.forward_recovery);
    assert_eq!(dm.monitor.awareness, 1.);
    assert_eq!(dm.interaction_grace_remaining(), 0.);
}
#[test]
fn touch_reconnection_requires_a_fresh_release_before_a_new_edge() {
    use openpilot_monitoring::context::SteeringTouchEvidence;
    let mut touch = SteeringTouchEvidence::default();
    assert_eq!(
        touch.update(1., true, true, 1_000_000_000, true, true),
        (true, false)
    );
    assert_eq!(
        touch.update(1.05, false, true, 1_050_000_000, true, true),
        (false, false)
    );
    assert_eq!(
        touch.update(1.1, true, true, 1_100_000_000, true, true),
        (true, false)
    );
    assert_eq!(
        touch.update(1.15, true, true, 1_150_000_000, false, true),
        (false, false)
    );
    assert_eq!(
        touch.update(1.2, true, true, 1_200_000_000, true, true),
        (true, true)
    );
    assert_eq!(
        touch.update(1.21, true, true, 1_200_000_000, true, true),
        (true, false)
    );
    assert_eq!(
        touch.update(1.5, true, true, 1_200_000_000, true, true),
        (false, false)
    );
}
#[test]
fn cancel_sequence_requires_three_consecutive_press_edges_within_three_seconds() {
    use openpilot_monitoring::context::CancelPressSequence;
    let mut seq = CancelPressSequence::default();
    let p = vec![("cancel".into(), true)];
    let r = vec![("cancel".into(), false)];
    assert!(!seq.update(1., &p));
    assert!(!seq.update(1.1, &p));
    seq.update(1.2, &r);
    assert!(!seq.update(2., &p));
    seq.update(2.1, &r);
    assert!(seq.update(4., &p));
    seq.reset_input_stream();
    seq.update(5., &p);
    seq.update(5.1, &r);
    seq.update(5.2, &[("lkas".into(), false)]);
    assert!(!seq.update(5.3, &p));
}
