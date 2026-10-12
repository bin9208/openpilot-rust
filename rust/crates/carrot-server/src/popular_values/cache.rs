//! Memory-only cache and scheduling policy from server/services/popular_values.py.
use crate::{json_fields::set, Error, Value};

pub fn empty(car_key: &str, settings_hash: &str) -> Value {
    Value::object([
        ("ok", Value::Bool(true)),
        ("source", Value::text("empty")),
        ("updated_at", Value::integer(0)),
        ("fetched_at", Value::integer(0)),
        ("car_key_type", Value::text("CarSelected3")),
        ("car_key", Value::text(car_key)),
        ("settings_hash", Value::text(settings_hash)),
        ("popular_values", Value::Object(Vec::new())),
    ])
}

fn text_or(value: &Value, fallback: &str) -> Result<String, Error> {
    if value.truth() {
        value.string().map_err(Error::from)
    } else {
        Ok(fallback.into())
    }
}

pub fn read(memory: Option<&Value>, car_key: &str, settings_hash: &str) -> Result<Value, Error> {
    let Some(data @ Value::Object(_)) = memory else {
        return Ok(empty(car_key, settings_hash));
    };
    let cached_car = text_or(data.get("car_key"), "")?;
    let cached_hash = text_or(data.get("settings_hash"), "")?;
    if (!car_key.is_empty() && !cached_car.is_empty() && car_key != cached_car)
        || (!settings_hash.is_empty() && !cached_hash.is_empty() && settings_hash != cached_hash)
    {
        return Ok(empty(car_key, settings_hash));
    }
    let mut result = data.clone();
    for (name, value) in [
        ("ok", Value::Bool(true)),
        ("source", Value::text("memory")),
        ("settings_hash", Value::text(settings_hash)),
        ("popular_values", Value::Object(Vec::new())),
    ] {
        if !result.has(name) {
            set(&mut result, name, value)?;
        }
    }
    Ok(result)
}

pub fn store(data: &Value, now: f64, settings_hash: &str) -> Result<Value, Error> {
    Ok(Value::object([
        (
            "ok",
            Value::Bool(if data.has("ok") {
                data.get("ok").truth()
            } else {
                true
            }),
        ),
        ("source", Value::text("remote")),
        ("fetched_at", Value::Float(now).int().map(Value::Integer)?),
        (
            "car_key_type",
            Value::text(&text_or(data.get("car_key_type"), "CarSelected3")?),
        ),
        ("car_key", Value::text(&text_or(data.get("car_key"), "")?)),
        (
            "settings_hash",
            Value::text(&text_or(data.get("settings_hash"), settings_hash)?),
        ),
        (
            "popular_values",
            match data.get("popular_values") {
                values @ Value::Object(_) => values.clone(),
                _ => Value::Object(Vec::new()),
            },
        ),
    ]))
}

pub fn detail(cache: &Value, name: &str) -> Value {
    match cache.get("popular_values").get(name) {
        value @ Value::Object(_) => value.clone(),
        _ => Value::object([("top_values", Value::Array(Vec::new()))]),
    }
}

pub fn should_schedule(session: bool, now: f64, last: f64, interval: f64, in_flight: bool) -> bool {
    if now - last < interval {
        return false;
    }
    session && !in_flight
}
