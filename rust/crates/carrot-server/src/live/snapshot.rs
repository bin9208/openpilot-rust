use super::value;
use crate::Value;
use capnp::dynamic_value::Reader;
pub(super) fn snapshot(name: &str, data: Reader<'_>) -> Value {
    match name {
        "selfdriveState" => Value::object([
            (
                "enabled",
                Value::Bool(value::truth(value::field(data, "enabled"))),
            ),
            (
                "experimentalMode",
                Value::Bool(value::truth(value::field(data, "experimentalMode"))),
            ),
            (
                "alertType",
                Value::text(&value::text(value::field(data, "alertType"))),
            ),
            (
                "alertText1",
                Value::text(&value::text(value::field(data, "alertText1"))),
            ),
            (
                "alertText2",
                Value::text(&value::text(value::field(data, "alertText2"))),
            ),
            (
                "alertStatus",
                Value::integer(value::integer(value::field(data, "alertStatus"))),
            ),
            (
                "alertSize",
                Value::integer(value::integer(value::field(data, "alertSize"))),
            ),
        ]),
        "navInstructionCarrot" => Value::object([
            ("mainText", optional_text(value::field(data, "mainText"))),
            (
                "distanceText",
                optional_text(value::field(data, "distanceText")),
            ),
            ("turnType", optional_text(value::field(data, "turnType"))),
        ]),
        "navRoute" => {
            let mut coordinates = Vec::new();
            if let Reader::List(items) = value::field(data, "coordinates") {
                for index in 0..items.len().min(2000) {
                    let Ok(item) = items.get(index) else {
                        coordinates.clear();
                        break;
                    };
                    let lat = value::number(value::field(item, "latitude"), f64::NAN);
                    let lon = value::number(value::field(item, "longitude"), f64::NAN);
                    if lat.abs() > 90. || lon.abs() > 180. || (lat == 0. && lon == 0.) {
                        continue;
                    }
                    coordinates.push(Value::object([
                        ("lat", Value::Float(lat)),
                        ("lon", Value::Float(lon)),
                    ]));
                }
            }
            Value::object([
                ("count", Value::integer(coordinates.len())),
                ("coordinates", Value::Array(coordinates)),
            ])
        }
        _ => Value::object([]),
    }
}

fn optional_text(value: Reader<'_>) -> Value {
    match value {
        Reader::Void => Value::Null,
        value => Value::text(&super::value::text(value)),
    }
}
