use openpilot_logmessaged::{JsonValue, JsonView};
use openpilot_runtime_core::python_float::parse;
use serde_json::Value;

pub fn json(value: &Value) -> Result<JsonValue, crate::Error> {
    JsonValue::parse(&value.to_string()).map_err(|_| crate::Error::Invalid("invalid JSON value"))
}

pub fn python_number(value: &JsonValue) -> Result<Option<f64>, crate::Error> {
    Ok(match value.view() {
        JsonView::Null | JsonView::Array(_) | JsonView::Object(_) => None,
        JsonView::Bool(value) => Some(f64::from(u8::from(value))),
        JsonView::Float(value) => Some(value),
        JsonView::Integer(value) => {
            let number = value
                .parse::<f64>()
                .map_err(|_| crate::Error::IntegerFloatOverflow)?;
            if number.is_infinite() {
                return Err(crate::Error::IntegerFloatOverflow);
            }
            Some(number)
        }
        JsonView::Text(_) => value.to_utf8().and_then(|text| parse(&text)),
    })
}

pub fn python_integer(value: &JsonValue) -> Result<Option<f64>, crate::Error> {
    Ok(match value.view() {
        JsonView::Null | JsonView::Array(_) | JsonView::Object(_) => None,
        JsonView::Bool(value) => Some(f64::from(u8::from(value))),
        JsonView::Float(value) => {
            if value.is_infinite() {
                return Err(crate::Error::FloatOverflow);
            }
            (!value.is_nan()).then_some(value.trunc())
        }
        JsonView::Integer(value) => value.parse().ok(),
        JsonView::Text(_) => value.to_utf8().and_then(|text| integer_text(&text)),
    })
}

pub fn integer_text(value: &str) -> Option<f64> {
    if value.contains(['.', 'e', 'E']) || value.chars().filter(|c| c.is_numeric()).count() > 4300 {
        return None;
    }
    let result = parse(value)?;
    (!result.is_nan() && !value.to_ascii_lowercase().contains("inf")).then_some(result)
}
