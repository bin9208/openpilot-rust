use openpilot_logging::{diagnostics::Diagnostics, Fields, Number, Value};

fn values(items: &[(&str, Number)]) -> Vec<(String, Number)> {
    items
        .iter()
        .map(|(key, value)| ((*key).into(), *value))
        .collect()
}

#[test]
fn aggregation_filters_nonfinite_values_and_resets_even_when_context_collides() {
    let mut diagnostics = Diagnostics::new("modeld", 1.0, 0.0, Some([10, 20, 2]), Some(true), 17);
    assert!(diagnostics
        .record_with(
            values(&[("ms", Number::Float(1.2345))]),
            Fields::new(),
            || 0.5,
            || panic!("scheduler must only be sampled when flushing")
        )
        .unwrap()
        .is_none());
    let event = diagnostics
        .record_with(
            values(&[
                ("ms", Number::Float(2.5)),
                ("nan", Number::Float(f64::NAN)),
                ("published", Number::Integer(1)),
            ]),
            Fields::new(),
            || 1.0,
            || Some([110, 220, 5]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(event["frames"], Value::Integer(2));
    assert!(event.to_json().unwrap().contains("\"max\": 2.5"));
    assert!(!event.to_json().unwrap().contains("nan"));
    let mut collision = Fields::new();
    collision.insert("component".into(), Value::Text("collision".into()));
    assert!(diagnostics
        .record_with(
            values(&[("ms", Number::Float(9.0))]),
            collision,
            || 2.0,
            || Some([210, 320, 7])
        )
        .unwrap()
        .is_none());
    let event = diagnostics
        .record_with(
            values(&[("ms", Number::Float(3.0))]),
            Fields::new(),
            || 3.0,
            || Some([310, 420, 9]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(event["frames"], Value::Integer(1));
    assert!(event.to_json().unwrap().contains("\"mean\": 3.0"));
}

#[test]
fn finite_input_overflow_keeps_python_nonfinite_json_and_integer_maxima() {
    let mut diagnostics = Diagnostics::new("probe", 0.0, 0.0, None, None, 1);
    let event = diagnostics
        .record_with(
            values(&[("count", Number::Integer(4))]),
            Fields::new(),
            || 1.0,
            || None,
        )
        .unwrap()
        .unwrap();
    assert!(event.to_json().unwrap().contains("\"max\": 4,"));
    let mut diagnostics = Diagnostics::new("probe", 1.0, 0.0, None, None, 1);
    diagnostics
        .record_with(
            values(&[("large", Number::Float(1e308))]),
            Fields::new(),
            || 0.5,
            || None,
        )
        .unwrap();
    let event = diagnostics
        .record_with(
            values(&[("large", Number::Float(1e308))]),
            Fields::new(),
            || 1.0,
            || None,
        )
        .unwrap()
        .unwrap();
    assert!(event.to_json().unwrap().contains("\"mean\": Infinity"));
}
