use crate::{Error, JsonValue, JsonView};
pub(crate) fn truthy(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value != "0" && value != "-0",
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}
pub(crate) fn get(value: &JsonValue, key: &str, default_json: &str) -> Result<JsonValue, Error> {
    if !value.is_object() {
        return Err(Error::Attribute("JSON value has no get method"));
    }
    match value.get(key) {
        Some(value) => Ok(value),
        None => Ok(JsonValue::parse(default_json)?),
    }
}
pub(crate) fn object<const N: usize>(fields: [(&str, JsonValue); N]) -> Result<JsonValue, Error> {
    let entries: Result<Vec<_>, Error> = fields
        .into_iter()
        .map(|(key, value)| {
            Ok(format!(
                "{}:{}",
                JsonValue::text(key).to_json()?,
                value.to_json()?
            ))
        })
        .collect();
    Ok(JsonValue::parse(&format!("{{{}}}", entries?.join(",")))?)
}
pub(crate) fn array(values: &[JsonValue]) -> Result<JsonValue, Error> {
    let items: Result<Vec<_>, _> = values.iter().map(JsonValue::to_json).collect();
    Ok(JsonValue::parse(&format!("[{}]", items?.join(",")))?)
}
pub(crate) fn upper(value: &JsonValue) -> Result<JsonValue, Error> {
    let JsonView::Text(points) = value.view() else {
        return Err(Error::Attribute("JSON value has no upper method"));
    };
    let mut result = Vec::new();
    for &point in points {
        match char::from_u32(point) {
            // CPython 3.12 (Unicode 15) leaves these code points unchanged;
            // their uppercase mappings were added in Unicode 16.
            Some('\u{0264}' | '\u{1c8a}') => result.push(point),
            Some(character) => result.extend(character.to_uppercase().map(u32::from)),
            None => result.push(point),
        }
    }
    JsonValue::codepoints(result).ok_or(Error::Value("invalid Unicode code point"))
}
