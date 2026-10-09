use super::Config;
use crate::{json_fields::set, Error, Value};
use std::fs;

pub(super) fn read(config: &Config) -> Value {
    crate::state::read(&config.state)
}
pub(super) fn now() -> f64 {
    chrono::Utc::now()
        .timestamp_micros()
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.)
        / 1e6
}
pub(super) fn write(config: &Config, state: &mut Value) -> Result<(), Error> {
    set(state, "updated_at", Value::Float(now()))?;
    let Value::Object(fields) = state else {
        return Err(Error::Source("vision state is not an object".into()));
    };
    fields.sort_by(|(first, _), (second, _)| first.cmp(second));
    let temporary = config.state.with_extension("tmp");
    fs::write(&temporary, state.encode()?)
        .map_err(|error| crate::state::io_error(error, &temporary))?;
    fs::rename(&temporary, &config.state)
        .map_err(|error| crate::state::io_error(error, &temporary))?;
    Ok(())
}
pub(super) fn text(value: &Value) -> String {
    if value.truth() {
        value.string().unwrap_or_default()
    } else {
        String::new()
    }
}
pub(super) fn tail(config: &Config, lines: usize) -> Vec<String> {
    let bytes = fs::read(&config.log).unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    let rows: Vec<_> = text.lines().map(str::to_owned).collect();
    rows[rows.len().saturating_sub(lines)..].to_vec()
}
