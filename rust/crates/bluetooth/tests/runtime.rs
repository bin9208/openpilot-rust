use indexmap::IndexMap;
use openpilot_bluetooth::{Address, Config, Engine, Event, Seconds, VehicleSnapshot};

#[test]
fn disengagement_revokes_pending_hold_but_allows_a_new_short_command() {
    let root = tempfile::tempdir().unwrap();
    let config = Config::parse(r#"{"devices":{"00:11:22:33:44:55":{"profile":"generic","enabled":true,"mapping":{"key:115":"accelCruise","key:115@long":"accelCruiseLong"}}}}"#).unwrap();
    let address = Address::parse("00:11:22:33:44:55").unwrap();
    let mut engine = Engine::new(root.path(), Config::default()).unwrap();
    let available = IndexMap::from([("input-0".to_owned(), address.clone())]);
    engine.reload(config, None, &available, Seconds(10.0));
    engine.connect("input-0", &address).unwrap();
    let mut state = VehicleSnapshot {
        device_alive: true,
        started: true,
        car_alive: true,
        car_valid: true,
        can_valid: true,
        controls_alive: true,
        enabled: true,
        gear_drive: true,
        ..VehicleSnapshot::default()
    };
    engine.update(state);
    key(&mut engine, 1, 10.0);
    engine
        .flush("input-0", Seconds(10.71), Seconds(10.71))
        .unwrap();
    assert_eq!(commands(root.path())[0]["action"], "accelCruiseLong");
    state.enabled = false;
    engine.update(state);
    engine
        .flush("input-0", Seconds(11.22), Seconds(11.22))
        .unwrap();
    engine.prune(Seconds(11.22)).unwrap();
    assert!(commands(root.path()).is_empty());
    key(&mut engine, 0, 11.23);
    engine.update(state);
    key(&mut engine, 1, 11.3);
    key(&mut engine, 0, 11.4);
    assert_eq!(commands(root.path())[0]["action"], "accelCruise");
}

fn key(engine: &mut Engine, value: i32, at: f64) {
    for event in [
        Event {
            kind: 1,
            code: 115,
            value,
            at: Seconds(at),
        },
        Event {
            kind: 0,
            code: 0,
            value: 0,
            at: Seconds(at),
        },
    ] {
        engine.event("input-0", event, Seconds(at)).unwrap();
    }
}

fn commands(root: &std::path::Path) -> Vec<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("cruise.json")).unwrap()).unwrap();
    value["events"].as_array().unwrap().clone()
}
