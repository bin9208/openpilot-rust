use openpilot_messaging::{
    frequency::FrequencyTracker,
    state::{Options, State},
};

#[test]
fn radar_ranges_and_conflated_poll_rates_match_source() {
    for frequency in [14.0, 16.0, 20.0, 25.0] {
        let mut tracker =
            FrequencyTracker::new(20.0, 100.0, false, None, Some((14.0, 25.0))).unwrap();
        for frame in 0..250 {
            tracker.record(f64::from(frame) / frequency);
        }
        assert!(tracker.valid().unwrap());
    }
    let tracker = FrequencyTracker::new(20.0, 20.0, false, None, Some((14.0, 25.0))).unwrap();
    assert_eq!(tracker.min_frequency, 14.0 * 0.8);
    assert_eq!(tracker.max_frequency, 20.0 * 1.2);
}

#[test]
fn repeated_receive_times_do_not_silently_bypass_frequency_failure() {
    let mut tracker = FrequencyTracker::new(20.0, 20.0, true, None, None).unwrap();
    tracker.record(1.0);
    tracker.record(1.0);
    assert!(tracker.valid().is_err());
}

#[test]
fn on_demand_defaults_and_stale_static_services_preserve_checks() {
    let mut state = State::new(&["carState", "carrotMan"], Options::default()).unwrap();
    assert_eq!(state.frame(), -1);
    assert!(!state.topic("carState").unwrap().seen);
    assert!(!state.topic("carState").unwrap().valid);
    assert!(state.topic("carrotMan").unwrap().valid);
    state.update(100.0, &[]).unwrap();
    assert!(!state.topic("carState").unwrap().alive);
    assert!(state.topic("carrotMan").unwrap().alive);
    assert!(state.all_checks(&["carrotMan"]).unwrap());
    assert!(!state.all_checks(&[]).unwrap());
}

#[test]
fn source_services_preserve_readable_payloads_and_known_schema_errors() {
    for service in openpilot_messaging::services::SERVICES {
        let result = State::new(&[service.name], Options::default());
        if matches!(
            service.name,
            "navModel" | "customReservedRawData1" | "customReservedRawData2"
        ) {
            assert!(
                matches!(result, Err(openpilot_messaging::state::Error::Cereal(error)) if error.kind == capnp::ErrorKind::FieldNotFound)
            );
            continue;
        }
        let state = result.unwrap();
        assert!(!state
            .topic(service.name)
            .unwrap()
            .event()
            .unwrap()
            .get_valid());
        assert!(
            state.topic(service.name).unwrap().data().is_ok(),
            "{}",
            service.name
        );
    }
}

#[test]
fn oversized_segment_header_is_rejected_before_state_advances() {
    let mut state = State::new(&["carState"], Options::default()).unwrap();
    let bytes = [0_u32.to_le_bytes(), 100_u32.to_le_bytes()].concat();
    let error = state.update(1.0, &[bytes]).unwrap_err();
    assert!(
        matches!(error, openpilot_messaging::state::Error::Cereal(error) if matches!(error.kind, capnp::ErrorKind::MessageTooLarge(100)))
    );
    assert_eq!(state.frame(), -1);
}
