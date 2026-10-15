use openpilot_cereal::log_capnp::onroad_event::EventName;
use openpilot_selfdrived::alerts::Alert;
use openpilot_selfdrived::events::{Catalog, CreateAlertError, Events};
use openpilot_selfdrived::state::EventType;

#[test]
fn both_hardware_catalogs_decode_with_all_callbacks_and_schema_enums() {
    for mici in [false, true] {
        let catalog = Catalog::load(mici).unwrap();
        assert_eq!(catalog.definitions().count(), 130);
        for entry in catalog.definitions() {
            assert!(!entry.name.is_empty());
            assert_eq!(catalog.get(entry.event).unwrap().name, entry.name);
        }
    }
}

#[test]
fn unknown_schema_values_and_missing_definitions_do_not_create_alerts() {
    let mut encoded = serde_json::to_value(Alert::default()).unwrap();
    encoded["audible_alert"] = serde_json::json!(65535);
    assert!(serde_json::from_value::<Alert>(encoded).is_err());
    let mut events = Events::new(Catalog::load(false).unwrap());
    events.add(EventName::SoundsUnavailableDEPRECATED, false);
    assert!(!events.contains(EventType::Warning));
    let result = events.create_alerts(&[], |_| Ok::<_, &str>(Alert::default()), str::to_owned);
    assert!(matches!(
        result,
        Err(CreateAlertError::UndefinedEvent(
            EventName::SoundsUnavailableDEPRECATED
        ))
    ));
}

#[test]
fn callback_errors_propagate_without_substituting_an_empty_alert() {
    let mut events = Events::new(Catalog::load(false).unwrap());
    events.add(EventName::JoystickDebug, false);
    let result = events.create_alerts(&[EventType::Warning], |_| Err("unavailable"), str::to_owned);
    assert!(matches!(
        result,
        Err(CreateAlertError::Callback("unavailable"))
    ));
}

#[test]
fn historical_dm_fallback_event_maps_by_name_and_has_no_warning_banner() {
    let catalog = Catalog::load(false).unwrap();
    let entry = catalog.get(EventName::DriverMonitorFallback).unwrap();
    assert_eq!(entry.name, "driverMonitorFallback");
    assert!(entry.definitions.is_empty());
    assert_eq!(u16::from(entry.event), 130);
}
#[test]
fn temporary_steer_warning_waits_twelve_completed_fault_cycles_without_delaying_no_entry() {
    let mut events = Events::new(Catalog::load(false).unwrap());
    events.add(EventName::SteerTempUnavailableSilent, false);
    let alerts = events
        .create_alerts(
            &[EventType::Warning],
            |_| Ok::<_, &str>(Alert::default()),
            str::to_owned,
        )
        .unwrap();
    assert!(
        alerts.is_empty(),
        "noncritical presentation must wait for the 120ms fault history"
    );
    for _ in 0..12 {
        events.clear();
        events.add(EventName::SteerTempUnavailableSilent, false);
    }
    let alerts = events
        .create_alerts(
            &[EventType::Warning],
            |_| Ok::<_, &str>(Alert::default()),
            str::to_owned,
        )
        .unwrap();
    assert_eq!(alerts.len(), 1);
    events.clear();
    events.add(EventName::SteerTempUnavailable, false);
    assert!(events.contains(EventType::NoEntry));
    let alerts = events
        .create_alerts(
            &[EventType::NoEntry],
            |_| Ok::<_, &str>(Alert::default()),
            str::to_owned,
        )
        .unwrap();
    assert!(!alerts.is_empty());
}

#[test]
fn temporary_steering_history_survives_short_healthy_gaps_and_resets_after_one_second() {
    let mut events = Events::new(Catalog::load(false).unwrap());
    let shown = |events: &Events| {
        !events
            .create_alerts(
                &[EventType::Warning],
                |_| Ok::<_, &str>(Alert::default()),
                str::to_owned,
            )
            .unwrap()
            .is_empty()
    };
    for _ in 0..6 {
        events.add(EventName::SteerTempUnavailableSilent, false);
        events.clear();
    }
    for _ in 0..20 {
        events.clear();
    }
    for _ in 0..6 {
        events.add(EventName::SteerTempUnavailableSilent, false);
        events.clear();
    }
    events.add(EventName::SteerTempUnavailableSilent, false);
    assert!(shown(&events));
    events.clear();
    for _ in 0..100 {
        events.clear();
    }
    events.add(EventName::SteerTempUnavailableSilent, false);
    assert!(!shown(&events));
}
