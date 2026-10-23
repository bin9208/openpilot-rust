use super::value;
use crate::Value;
use capnp::dynamic_value::Reader;

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod snapshot_tests;
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
        "navInstructionCarrot" => {
            // Cereal distances and the browser's distanceM contract both use meters.
            let meters = value::number(value::field(data, "maneuverDistance"), f64::NAN);
            let valid_distance = meters.is_finite() && meters >= 0.;
            Value::object([
                (
                    "mainText",
                    optional_text(value::field(data, "maneuverPrimaryText")),
                ),
                (
                    "distanceText",
                    if valid_distance {
                        Value::text(&distance_text(meters))
                    } else {
                        Value::Null
                    },
                ),
                (
                    "distanceM",
                    if valid_distance {
                        Value::Float(meters)
                    } else {
                        Value::Null
                    },
                ),
                (
                    "turnType",
                    optional_text(value::field(data, "maneuverType")),
                ),
            ])
        }
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

fn distance_text(meters: f64) -> String {
    if meters >= 10_000. {
        format!("{:.0}km", meters / 1000.)
    } else if meters >= 1000. {
        format!("{:.1}km", meters / 1000.)
    } else {
        format!("{:.0}m", meters.round())
    }
}

fn optional_text(value: Reader<'_>) -> Value {
    match value {
        Reader::Void => Value::Null,
        value => Value::text(&super::value::text(value)),
    }
}
