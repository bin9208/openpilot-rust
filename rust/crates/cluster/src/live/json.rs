use num_traits::ToPrimitive;
use openpilot_logmessaged::{JsonValue, JsonView};

pub(super) fn read(params: &openpilot_params::Params, key: &str) -> Option<JsonValue> {
    let bytes = params.get(key).ok().flatten()?;
    JsonValue::parse(std::str::from_utf8(&bytes).ok()?)
        .ok()
        .filter(JsonValue::is_object)
}
pub(super) fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value != "0",
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}
pub(super) fn text(value: &JsonValue) -> Option<String> {
    openpilot_runtime_version::python_str(value)
        .ok()?
        .into_iter()
        .map(char::from_u32)
        .collect()
}
pub(super) fn field_text(value: &JsonValue, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .filter(truth)
        .and_then(|value| text(&value))
        .unwrap_or_else(|| fallback.into())
}
pub(super) fn finite(value: &JsonValue) -> Option<f64> {
    let number = match value.view() {
        JsonView::Bool(value) => f64::from(value),
        JsonView::Float(value) => value,
        JsonView::Integer(value) => value.parse().ok()?,
        JsonView::Text(_) => openpilot_hardware_info::parse_float(&value.to_utf8()?).ok()?,
        JsonView::Null | JsonView::Array(_) | JsonView::Object(_) => return None,
    };
    number.is_finite().then_some(number)
}
pub(super) fn positive(value: Option<JsonValue>, maximum: i64) -> Option<i64> {
    let number = finite(&value?)?.trunc().to_i64()?;
    (number > 0 && number <= maximum).then_some(number)
}
pub(super) fn optional_bool(value: Option<JsonValue>) -> Option<bool> {
    let value = value?;
    match value.view() {
        JsonView::Bool(value) => Some(value),
        JsonView::Integer(value) => Some(value != "0"),
        JsonView::Float(value) if value.is_finite() => Some(value.trunc() != 0.0),
        JsonView::Text(_) => {
            let text = value.to_utf8()?;
            let text = openpilot_startup_ui::text::trim(&text).to_lowercase();
            match text.as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => openpilot_ui_application::params::typed::integer_text(&text)
                    .map(|value| value != "0"),
            }
        }
        JsonView::Null | JsonView::Float(_) | JsonView::Array(_) | JsonView::Object(_) => None,
    }
}
pub(super) fn truncate(text: &str, count: usize) -> String {
    text.chars().take(count).collect()
}
