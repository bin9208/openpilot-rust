use crate::{Error, Value};

pub(crate) fn fields(value: &Value) -> Result<&Vec<(Vec<u32>, Value)>, Error> {
    match value {
        Value::Object(fields) => Ok(fields),
        value => Err(Error::Source(format!(
            "'{}' object has no attribute 'get'",
            value.type_name()
        ))),
    }
}

pub(crate) fn array(value: &Value) -> Result<&Vec<Value>, Error> {
    match value {
        Value::Array(items) => Ok(items),
        value => Err(Error::Source(format!(
            "'{}' object is not iterable",
            value.type_name()
        ))),
    }
}

pub(crate) fn set(value: &mut Value, name: &str, replacement: Value) -> Result<(), Error> {
    let Value::Object(fields) = value else {
        return Err(Error::Source(format!(
            "'{}' object does not support item assignment",
            value.type_name()
        )));
    };
    let key: Vec<u32> = name.chars().map(u32::from).collect();
    insert(fields, key, replacement);
    Ok(())
}

pub(crate) fn insert(fields: &mut Vec<(Vec<u32>, Value)>, name: Vec<u32>, value: Value) {
    if let Some((_, previous)) = fields.iter_mut().find(|(key, _)| *key == name) {
        *previous = value;
    } else {
        fields.push((name, value));
    }
}

pub(crate) fn field<'a>(fields: &'a [(Vec<u32>, Value)], name: &[u32]) -> Option<&'a Value> {
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

pub(crate) fn key(value: &Value) -> Result<Vec<u32>, Error> {
    match value {
        Value::Text(points) => Ok(points.clone()),
        Value::Integer(_) | Value::Float(_) | Value::Bool(_) | Value::Null => {
            Ok(value.string()?.chars().map(u32::from).collect())
        }
        Value::Array(_) | Value::Object(_) => Err(Error::Source(format!(
            "unhashable type: '{}'",
            value.type_name()
        ))),
    }
}
