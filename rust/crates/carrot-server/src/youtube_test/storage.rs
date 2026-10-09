use super::config::Config;
use crate::{Error, Value};
use num_traits::ToPrimitive;
use std::{fs, path::Path};

pub(super) fn json(path: &Path) -> Value {
    crate::state::read(path)
}
pub(super) fn text(value: &Value) -> String {
    if value.truth() {
        value
            .py_string()
            .and_then(|text| text.string())
            .unwrap_or_default()
    } else {
        String::new()
    }
}
pub(super) fn integer(value: &Value) -> Result<i64, Error> {
    if !value.truth() {
        return Ok(0);
    }
    value
        .int()?
        .to_i64()
        .ok_or_else(|| Error::Source("test integer out of range".into()))
}
pub(super) fn configured(value: &Value) -> bool {
    if !value.truth() {
        return false;
    }
    value.py_string().ok().is_some_and(|value| match value {
        Value::Text(points) => !crate::state::trim(&points).is_empty(),
        _ => false,
    })
}
pub(super) fn set(object: &mut Value, key: &str, value: Value) {
    let points: Vec<_> = key.chars().map(u32::from).collect();
    if let Value::Object(fields) = object {
        if let Some((_, current)) = fields.iter_mut().find(|(name, _)| *name == points) {
            *current = value;
        } else {
            fields.push((points, value));
        }
    }
}
pub(super) fn monotonic() -> f64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    time.tv_sec.to_f64().unwrap_or(0.0) + time.tv_nsec.to_f64().unwrap_or(0.0) / 1e9
}
pub(super) fn write(config: &Config, state: &mut Value) -> Result<(), Error> {
    set(state, "updated_mono", Value::Float(monotonic()));
    let Value::Object(fields) = state else {
        return Err(Error::Source("test state is not an object".into()));
    };
    fields.sort_by(|(a, _), (b, _)| a.cmp(b));
    let temporary = config.paths.state.with_extension("tmp");
    fs::write(&temporary, state.encode()?)?;
    fs::rename(temporary, &config.paths.state)?;
    Ok(())
}
pub(super) fn tail(config: &Config, lines: usize) -> Vec<String> {
    let bytes = fs::read(&config.paths.log).unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    let rows: Vec<_> = text.lines().map(str::to_owned).collect();
    rows[rows.len().saturating_sub(lines)..].to_vec()
}
