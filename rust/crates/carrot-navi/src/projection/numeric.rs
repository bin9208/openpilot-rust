use crate::{json::Value, Error};
use num_bigint::BigInt;
use num_traits::{FromPrimitive, Zero};

pub(super) fn dict(value: &Value) -> &Value {
    if matches!(value, Value::Object(_)) {
        value
    } else {
        &Value::Null
    }
}
pub(super) fn list(value: &Value) -> &[Value] {
    match value {
        Value::Array(values) => values,
        _ => &[],
    }
}
pub(super) fn finite(
    value: &Value,
    default: f64,
    minimum: Option<f64>,
    maximum: Option<f64>,
) -> Result<f64, Error> {
    let mut parsed = match value.float() {
        Ok(value) => value,
        Err(error) if matches!(error.kind, "TypeError" | "ValueError") => return Ok(default),
        Err(error) => return Err(error),
    };
    if !parsed.is_finite() {
        return Ok(default);
    }
    if let Some(minimum) = minimum {
        parsed = if minimum < parsed { parsed } else { minimum };
    }
    if let Some(maximum) = maximum {
        parsed = if maximum > parsed { parsed } else { maximum };
    }
    Ok(parsed)
}
pub(super) fn integer(
    value: &Value,
    default: i64,
    minimum: Option<i64>,
    maximum: Option<i64>,
) -> Result<Value, Error> {
    let default = Value::integer(default).float()?;
    let number = finite(value, default, None, None)?;
    let mut parsed = BigInt::from_f64(number)
        .ok_or_else(|| Error::value("invalid finite integer conversion"))?;
    if let Some(minimum) = minimum {
        parsed = parsed.max(BigInt::from(minimum));
    }
    if let Some(maximum) = maximum {
        parsed = parsed.min(BigInt::from(maximum));
    }
    Ok(Value::Integer(parsed))
}
pub(super) fn integer0(value: &Value, maximum: i64) -> Result<Value, Error> {
    integer(value, 0, Some(0), Some(maximum))
}
pub(super) fn number0(value: &Value, maximum: f64) -> Result<Value, Error> {
    Ok(Value::Float(finite(value, 0., Some(0.), Some(maximum))?))
}
pub(super) fn text(value: &Value, maximum: usize) -> Result<Value, Error> {
    if !value.truth() {
        return Ok(Value::text(""));
    }
    match value.py_string()? {
        Value::Text(points) => Ok(Value::Text(points.into_iter().take(maximum).collect())),
        _ => Err(Error::value("invalid Python text conversion")),
    }
}
pub(super) fn record<'a>(snapshot: &'a Value, name: &str) -> &'a Value {
    dict(dict(snapshot.get("items")).get(name))
}
pub(super) fn present_value(record: &Value) -> &Value {
    if record.get("present").truth() {
        dict(record.get("value"))
    } else {
        &Value::Null
    }
}
pub(super) fn meta(record: &Value) -> Result<Value, Error> {
    Ok(Value::object([
        ("present", Value::Bool(record.get("present").truth())),
        (
            "sequence",
            integer(record.get("sequence"), 0, Some(0), None)?,
        ),
        (
            "sourceTimestampMillis",
            integer(record.get("source_timestamp_ms"), 0, Some(0), None)?,
        ),
        (
            "receivedMonoTimeNanos",
            integer(record.get("received_mono_ns"), 0, Some(0), None)?,
        ),
    ]))
}
pub(super) fn valid_road_limit(value: &Value) -> Result<Option<Value>, Error> {
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    let Value::Integer(number) = integer(value, 0, None, None)? else {
        return Err(Error::value("invalid integer projection"));
    };
    Ok((number > BigInt::zero()
        && number <= BigInt::from(200)
        && (&number % BigInt::from(10)).is_zero())
    .then_some(Value::Integer(number)))
}
