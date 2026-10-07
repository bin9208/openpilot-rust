use super::numeric::*;
use crate::{json::Value, Error};

pub(super) struct Light {
    valid: bool,
    on: bool,
    remain: Value,
}

pub(super) fn lights(snapshot: &Value) -> Result<[Light; 5], Error> {
    let value = present_value(record(snapshot, "traffic_signal"));
    let lights = dict(value.get("lights"));
    fn light(value: &Value) -> Result<Light, Error> {
        let value = dict(value);
        if !value.truth() {
            return Ok(Light {
                valid: false,
                on: false,
                remain: Value::integer(0),
            });
        }
        Ok(Light {
            valid: true,
            on: value.get("on").truth(),
            remain: integer0(value.get("remain_sec"), 999)?,
        })
    }
    Ok([
        light(lights.get("red"))?,
        light(lights.get("left"))?,
        light(lights.get("green"))?,
        light(lights.get("right"))?,
        light(lights.get("uturn"))?,
    ])
}
pub(super) fn traffic(snapshot: &Value, lights: [Light; 5]) -> Result<Value, Error> {
    let record = record(snapshot, "traffic_signal");
    let value = present_value(record);
    let counter = dict(value.get("ui_counter"));
    let [red, left, green, right, uturn] = lights;
    Ok(Value::object([
        ("meta", meta(record)?),
        ("visible", Value::Bool(value.get("visible").truth())),
        ("distanceM", integer0(value.get("distance_m"), 100_000)?),
        ("source", text(value.get("source"), 32)?),
        ("redValid", Value::Bool(red.valid)),
        ("redOn", Value::Bool(red.on)),
        ("redRemainSec", red.remain),
        ("leftValid", Value::Bool(left.valid)),
        ("leftOn", Value::Bool(left.on)),
        ("leftRemainSec", left.remain),
        ("greenValid", Value::Bool(green.valid)),
        ("greenOn", Value::Bool(green.on)),
        ("greenRemainSec", green.remain),
        ("rightValid", Value::Bool(right.valid)),
        ("rightOn", Value::Bool(right.on)),
        ("rightRemainSec", right.remain),
        ("uturnValid", Value::Bool(uturn.valid)),
        ("uturnOn", Value::Bool(uturn.on)),
        ("uturnRemainSec", uturn.remain),
        (
            "uiCounterValid",
            Value::Bool(!matches!(counter.get("remain_sec"), Value::Null)),
        ),
        (
            "uiCounterRemainSec",
            integer0(counter.get("remain_sec"), 999)?,
        ),
    ]))
}
