use openpilot_cereal::log_capnp::onroad_event::EventName;
use openpilot_selfdrived::alerts::Alert;
use openpilot_selfdrived::events::{Catalog, CreateAlertError, Events};
use openpilot_selfdrived::state::EventType;

#[test]
fn both_hardware_catalogs_decode_with_all_callbacks_and_schema_enums() {
    for mici in [false, true] {
        let catalog = Catalog::load(mici).unwrap();
        assert_eq!(catalog.definitions().count(), 125);
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
    events.add(EventName::RadarStationaryLead, false);
    assert!(!events.contains(EventType::Warning));
    let result = events.create_alerts(&[], |_| Ok::<_, &str>(Alert::default()), str::to_owned);
    assert!(matches!(
        result,
        Err(CreateAlertError::UndefinedEvent(
            EventName::RadarStationaryLead
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
