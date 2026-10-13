use crate::{Error, Value};

pub(crate) fn string(value: &Value, empty_when_false: bool) -> Result<Value, Error> {
    if empty_when_false && !value.truth() {
        return Ok(Value::text(""));
    }
    Ok(value.py_string()?)
}
pub(crate) fn stripped(value: &Value, empty_when_false: bool) -> Result<Value, Error> {
    let Value::Text(points) = string(value, empty_when_false)? else {
        return Err(Error::Source("expected Python string".into()));
    };
    Ok(Value::Text(crate::state::trim(&points).to_vec()))
}

pub(crate) fn equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Float(left), Value::Float(right)) => left == right,
        (Value::Float(number), Value::Integer(_) | Value::Bool(_)) => {
            number.fract() == 0.
                && left
                    .int()
                    .is_ok_and(|number| right.int().is_ok_and(|other| number == other))
        }
        (Value::Integer(_) | Value::Bool(_), Value::Float(_)) => equal(right, left),
        (Value::Integer(_) | Value::Bool(_), Value::Integer(_) | Value::Bool(_)) => left
            .int()
            .is_ok_and(|number| right.int().is_ok_and(|other| number == other)),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(a, b)| equal(a, b))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right
                        .iter()
                        .find(|(other, _)| key == other)
                        .is_some_and(|(_, other)| equal(value, other))
                })
        }
        (Value::Null, Value::Null) => true,
        (Value::Text(left), Value::Text(right)) => left == right,
        (
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::Float(_)
            | Value::Text(_)
            | Value::Array(_)
            | Value::Object(_),
            _,
        ) => false,
    }
}
