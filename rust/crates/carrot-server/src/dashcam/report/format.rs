use crate::{Error, Value};
use chrono::{Local, TimeZone};
use num_traits::ToPrimitive;

pub fn round(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}").parse().unwrap_or(value)
}
pub fn hms(seconds: f64) -> Result<String, Error> {
    let seconds = seconds
        .round_ties_even()
        .max(0.0)
        .to_u64()
        .ok_or_else(|| Error::Source("cannot convert float to integer".into()))?;
    Ok(format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    ))
}
pub fn ms(seconds: f64) -> Result<String, Error> {
    let seconds = seconds
        .round_ties_even()
        .max(0.0)
        .to_u64()
        .ok_or_else(|| Error::Source("cannot convert float to integer".into()))?;
    Ok(format!("{:02}:{:02}", seconds / 60, seconds % 60))
}
pub fn clock(epoch: f64) -> Result<String, Error> {
    if epoch == 0.0 || epoch <= 0.0 {
        return Ok("-".into());
    }
    let seconds = epoch
        .to_i64()
        .ok_or_else(|| Error::Source("timestamp out of range".into()))?;
    let time = Local
        .timestamp_opt(seconds, 0)
        .single()
        .ok_or_else(|| Error::Source("timestamp out of range".into()))?;
    Ok(time.format("%H:%M:%S").to_string())
}
pub(super) fn number(value: f64, digits: usize) -> Value {
    Value::Float(round(value, digits))
}
