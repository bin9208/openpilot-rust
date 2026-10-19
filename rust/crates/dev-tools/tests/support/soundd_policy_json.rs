//! Preserve the unchanged Python capture's exact binary64 values when decoding.
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
pub fn parse(bytes: &[u8]) -> Value {
    exact(&JsonValue::parse(std::str::from_utf8(bytes).unwrap()).unwrap())
}
fn exact(value: &JsonValue) -> Value {
    match value.view() {
        JsonView::Null => Value::Null,
        JsonView::Bool(value) => Value::Bool(value),
        JsonView::Integer(value) if value.starts_with('-') => {
            Value::from(value.parse::<i64>().unwrap())
        }
        JsonView::Integer(value) => Value::from(value.parse::<u64>().unwrap()),
        JsonView::Float(value) => Value::from(value),
        JsonView::Text(_) => Value::String(value.to_utf8().unwrap()),
        JsonView::Array(values) => Value::Array(values.iter().map(exact).collect()),
        JsonView::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        key.iter().map(|c| char::from_u32(*c).unwrap()).collect(),
                        exact(value),
                    )
                })
                .collect(),
        ),
    }
}
