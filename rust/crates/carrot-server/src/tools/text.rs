use crate::{Error, Value};

pub(super) fn string(value: &Value, empty_when_false: bool) -> Result<String, Error> {
    crate::param_changes::text::stripped(value, empty_when_false)?
        .string()
        .map_err(Error::from)
}
pub(super) fn strip(value: &Value, default: &str) -> Result<String, Error> {
    if !value.truth() {
        return Ok(default.trim().into());
    }
    match value {
        Value::Text(points) => Value::Text(crate::state::trim(points).to_vec())
            .string()
            .map_err(Error::from),
        other => Err(Error::Source(format!(
            "'{}' object has no attribute 'strip'",
            other.type_name()
        ))),
    }
}
pub(super) fn float(value: &Value) -> f64 {
    value.float().unwrap_or(0.)
}
pub(super) fn time() -> f64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(time) => time.as_secs_f64(),
        Err(time) => -time.duration().as_secs_f64(),
    }
}
pub(super) fn trimmed_log(value: &Value) -> Value {
    let points = match value {
        Value::Text(points) => points.clone(),
        _ => Vec::new(),
    };
    Value::Text(points[points.len().saturating_sub(60_000)..].to_vec())
}
pub(super) fn lines(points: &[u32]) -> Vec<u32> {
    let mut normalized = Vec::with_capacity(points.len());
    let mut index = 0;
    while index < points.len() {
        let point = points[index];
        if point == 13 {
            normalized.push(10);
            if points.get(index + 1) == Some(&10) {
                index += 1;
            }
        } else {
            normalized.push(point);
        }
        index += 1;
    }
    normalized
}

pub(super) fn whitespace(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}
