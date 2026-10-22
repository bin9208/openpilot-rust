use super::{snapshot, value, Reader, Value};
use openpilot_cereal::log_capnp::event;

fn navigation(distance: f32, populated: bool) -> Value {
    let mut message = capnp::message::Builder::new_default();
    let root = message.init_root::<event::Builder<'_>>();
    let mut nav = root.init_nav_instruction_carrot();
    if populated {
        nav.set_maneuver_primary_text("우회전 / Main Street");
        nav.set_maneuver_type("turn");
    }
    nav.set_maneuver_distance(distance);
    let bytes = capnp::serialize::write_message_to_words(&message);
    let decoded = value::message(&bytes).unwrap();
    snapshot(
        "navInstructionCarrot",
        value::service(&decoded, "navInstructionCarrot").unwrap(),
    )
}

#[test]
fn populated_cereal_navigation_maps_to_browser_fields_in_meters() {
    let result = navigation(35.5, true);
    assert_eq!(
        result,
        Value::object([
            ("mainText", Value::text("우회전 / Main Street")),
            ("distanceText", Value::text("36m")),
            ("distanceM", Value::Float(35.5)),
            ("turnType", Value::text("turn")),
        ])
    );
    // The response must also remain JSON encodable.
    assert_eq!(Value::parse(&result.encode().unwrap()).unwrap(), result);
}

#[test]
fn navigation_distance_text_uses_metric_units_without_losing_numeric_precision() {
    for (meters, expected) in [
        (0.0, "0m"),
        (999.0, "999m"),
        (1000.0, "1.0km"),
        (1200.0, "1.2km"),
        (10000.0, "10km"),
    ] {
        let result = navigation(meters, true);
        assert_eq!(result.get("distanceText"), &Value::text(expected));
        assert_eq!(result.get("distanceM"), &Value::Float(f64::from(meters)));
    }
}

#[test]
fn absent_navigation_fields_remain_optional_and_empty_cereal_text_stays_empty() {
    assert_eq!(
        snapshot("navInstructionCarrot", Reader::Void),
        Value::object([
            ("mainText", Value::Null),
            ("distanceText", Value::Null),
            ("distanceM", Value::Null),
            ("turnType", Value::Null),
        ])
    );
    let empty = navigation(0.0, false);
    assert_eq!(empty.get("mainText"), &Value::text(""));
    assert_eq!(empty.get("turnType"), &Value::text(""));
    for meters in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let result = navigation(meters, true);
        assert_eq!(result.get("distanceM"), &Value::Null);
        assert_eq!(result.get("distanceText"), &Value::Null);
        assert_eq!(result.get("turnType"), &Value::text("turn"));
        result.encode().unwrap();
    }
}
