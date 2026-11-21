use crate::{Error, Value};
use num_bigint::BigInt;
use num_traits::FromPrimitive;

pub fn infer_type(setting: &Value) -> &'static str {
    let (minimum, maximum, default) = (
        setting.get("min"),
        setting.get("max"),
        setting.get("default"),
    );
    if minimum.number_eq(0)
        && maximum.number_eq(1)
        && (default.number_eq(0) || default.number_eq(1))
    {
        "bool"
    } else if [minimum, maximum, default]
        .iter()
        .all(|value| matches!(value, Value::Integer(_) | Value::Bool(_)))
    {
        "int"
    } else if [minimum, maximum, default]
        .iter()
        .all(|value| matches!(value, Value::Integer(_) | Value::Bool(_) | Value::Float(_)))
    {
        "float"
    } else {
        "string"
    }
}

pub(crate) fn bool_value(value: &Value) -> Result<bool, Error> {
    if matches!(value, Value::Text(_)) {
        Ok(matches!(
            value.string()?.trim().to_lowercase().as_str(),
            "1" | "true" | "on" | "yes"
        ))
    } else {
        Ok(value.truth())
    }
}

pub(crate) fn rounded(value: &Value) -> Result<BigInt, Error> {
    let number = python_float(value)?.round_ties_even();
    BigInt::from_f64(number).ok_or_else(|| {
        Error::Source(if number.is_nan() {
            "cannot convert float NaN to integer".into()
        } else {
            "cannot convert float infinity to integer".into()
        })
    })
}

pub(crate) fn python_float(value: &Value) -> Result<f64, Error> {
    value.float().map_err(|error| {
        if matches!(value, Value::Text(_)) && error.kind == "ValueError" {
            Error::Source(format!(
                "could not convert string to float: {}",
                value.repr().unwrap_or_default()
            ))
        } else {
            Error::Json(error)
        }
    })
}

/// The tail of carrot's `clamp_numeric`: on a setting where 0 means "off" and
/// real values start at `nonzero_min`, a value between the two becomes that
/// minimum. A `nonzero_min` that is not a number changes nothing.
pub(crate) fn lift_to_nonzero_min(number: f64, setting: &Value) -> f64 {
    match setting.get("nonzero_min") {
        Value::Null => number,
        bound => match bound.float() {
            Ok(minimum) if 0. < number && number < minimum => minimum,
            _ => number,
        },
    }
}

pub fn coerce_inferred(value: &Value, setting: &Value) -> Result<(&'static str, Value), Error> {
    let kind = infer_type(setting);
    let coerced = match kind {
        "bool" => Value::Bool(bool_value(value)?),
        "int" => Value::Integer(rounded(value)?),
        "float" => Value::Float(python_float(value)?),
        _ => value.py_string()?,
    };
    Ok((kind, coerced))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_between_zero_and_nonzero_min_becomes_that_minimum() {
        // carrot_settings.json CenterToFrontRatio: 0 is "vehicle default", else 31-69.
        let setting =
            Value::parse(r#"{"min": 0, "max": 69, "default": 0, "nonzero_min": 31}"#).unwrap();
        for (given, expected) in [
            (0., 0.),
            (1., 31.),
            (30.9, 31.),
            (31., 31.),
            (45., 45.),
            (-3., -3.),
        ] {
            assert_eq!(lift_to_nonzero_min(given, &setting), expected, "{given}");
        }
        let plain = Value::parse(r#"{"min": 0, "max": 69, "default": 0}"#).unwrap();
        assert_eq!(lift_to_nonzero_min(12., &plain), 12.);
        let unusable = Value::parse(r#"{"nonzero_min": "thirty"}"#).unwrap();
        assert_eq!(lift_to_nonzero_min(12., &unusable), 12.);
    }
}
