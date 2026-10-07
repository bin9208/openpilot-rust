use crate::Error;
use serde_json::Value;

pub(crate) fn field<'a>(value: &'a Value, name: &'static str) -> Result<&'a Value, Error> {
    value.get(name).ok_or(Error::Field(name))
}

pub(crate) fn number(value: &Value, name: &'static str) -> Result<f64, Error> {
    match value {
        Value::Bool(value) => Ok(f64::from(u8::from(*value))),
        _ => value.as_f64().ok_or(Error::Field(name)),
    }
}

pub(crate) fn text(value: &Value, name: &'static str) -> Result<String, Error> {
    value.as_str().map(str::to_owned).ok_or(Error::Field(name))
}

pub(crate) fn optional_text(value: &Value, name: &'static str) -> Result<Option<String>, Error> {
    value
        .get(name)
        .filter(|item| !item.is_null())
        .map(|item| text(item, name))
        .transpose()
}
