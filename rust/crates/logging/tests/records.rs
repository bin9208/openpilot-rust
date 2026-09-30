use openpilot_logging::{
    record::{event, format_record, Level, Metadata},
    Fields, Value,
};

fn metadata() -> Metadata {
    Metadata {
        pathname: "/source/module.rs".into(),
        lineno: 10,
        module: "module".into(),
        function: "run".into(),
        host: "test-host".into(),
        process: 123,
        thread: 456,
        thread_name: "worker".into(),
        created: 1234.5,
    }
}

#[test]
fn event_key_presence_preserves_severity_and_fields() {
    let mut fields = Fields::new();
    fields.insert("debug".into(), Value::Bool(true));
    fields.insert("error".into(), Value::Bool(false));
    let (level, message) = event("event-name", vec![Value::Integer(1)], fields).unwrap();
    assert_eq!(level, Level::Error);
    let mut context = Fields::new();
    context.insert("version".into(), Value::Text("rust-test".into()));
    let packet = format_record(level, message, context, None, &metadata()).unwrap();
    assert_eq!(packet[0], 40);
    let value: serde_json::Value = serde_json::from_slice(&packet[1..]).unwrap();
    assert_eq!(value["msg"]["event"], "event-name");
    assert_eq!(value["msg"]["error"], false);
    assert_eq!(value["level"], "ERROR");
    assert_eq!(value["filename"], "module.rs");
    assert_eq!(value["ctx"]["version"], "rust-test");
}

#[test]
fn duplicate_event_argument_fails_and_nonfinite_fields_keep_source_encoding() {
    let mut fields = Fields::new();
    fields.insert("event".into(), Value::Text("duplicate".into()));
    assert!(event("original", vec![], fields).is_err());
    let packet = format_record(
        Level::Info,
        Value::Float(f64::NAN),
        Fields::new(),
        Some("Rust error details"),
        &metadata(),
    )
    .unwrap();
    let text = std::str::from_utf8(&packet[1..]).unwrap();
    assert!(text.contains("\"msg\": NaN"));
    assert!(text.contains("\"exc_info\": \"Rust error details\""));
}
