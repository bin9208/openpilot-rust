use openpilot_carrot_navi::{
    json::Value,
    manifest::{MapConfig, CATALOG},
    receiver::{Receiver, Stream},
    record::Clock,
};

#[derive(Default)]
struct TestClock;
impl Clock for TestClock {
    fn wall_ms(&mut self) -> i128 {
        1000
    }
    fn mono_ns(&mut self) -> u128 {
        2000
    }
}
fn negotiated() -> Receiver {
    let mut receiver = Receiver::new(Value::integer(7714), MapConfig::default());
    let query = Value::object([
        ("type", Value::text("requirements_query")),
        ("protocol_version", Value::integer(2)),
        ("catalog_revision", Value::integer(1)),
        (
            "streams",
            Value::Array(
                CATALOG
                    .iter()
                    .map(|&(kind, name)| {
                        Value::object([
                            ("kind", Value::text(kind)),
                            ("name", Value::text(name)),
                            ("schema_version", Value::integer(1)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    receiver
        .negotiate(&query, "fixture", || Ok("session".into()))
        .unwrap();
    receiver
}

#[test]
fn binary_input_advances_media_without_advancing_json_generation() {
    let mut receiver = negotiated();
    let before = receiver.cereal_snapshot().get("generation").clone();
    let metadata = Value::object([
        ("message_type", Value::integer(2)),
        ("format_or_reason", Value::integer(3)),
        ("manifest_revision", Value::integer(1)),
        ("stream_handle", Value::integer(28)),
        ("sequence", Value::integer(1)),
        ("source_timestamp_ms", Value::integer(1000)),
        ("flags", Value::integer(0)),
        ("width", Value::integer(32)),
        ("height", Value::integer(24)),
    ]);
    receiver
        .record_binary(
            Stream {
                session: "session",
                kind: "render",
                name: "map_main",
                peer: "peer",
            },
            &metadata,
            b"config",
            &mut TestClock,
        )
        .unwrap();
    assert_eq!(receiver.cereal_snapshot().get("generation"), &before);
    assert!(receiver
        .dashboard_snapshot()
        .metadata
        .get("media_generation")
        .number_eq(1));
    assert_eq!(
        receiver.media_bootstrap()[0].payload(),
        Some(b"config".as_slice())
    );
}

#[test]
fn disconnect_at_zero_still_changes_observable_generation() {
    let mut receiver = negotiated();
    receiver.control_disconnected();
    assert!(receiver.health().get("control_connections").number_eq(0));
    assert!(receiver.health().get("state_generation").number_eq(2));
}

#[test]
fn missing_manifest_identity_has_source_failure_order() {
    let receiver = negotiated();
    let identity = Value::object([("schema_version", Value::integer(1))]);
    let rejected = receiver
        .stream_config("session", "json", "vehicle", &identity)
        .unwrap_err();
    assert_eq!(rejected.message, "stale v2 manifest revision");
}
