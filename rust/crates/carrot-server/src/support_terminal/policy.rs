//! Input policy from services/support_terminal.py; durations remain the source allowlists.
use crate::{Error, Value};
use num_traits::ToPrimitive;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Permission {
    ApproveEach,
    AllowAll,
}
impl Permission {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ApproveEach => "approve_each",
            Self::AllowAll => "allow_all",
        }
    }
}
pub struct StartOptions {
    pub(super) note: Value,
    pub(super) ttl: u64,
    pub(super) permission: Permission,
    pub(super) command_timeout: u64,
}
pub(super) fn string(value: &Value) -> Result<Value, Error> {
    if value.truth() {
        Ok(value.py_string()?)
    } else {
        Ok(Value::text(""))
    }
}
pub(super) fn clipped(value: Value, limit: usize) -> Value {
    match value {
        Value::Text(points) => Value::Text(points.into_iter().take(limit).collect()),
        other => other,
    }
}
pub(super) fn stripped(value: &Value) -> Result<String, Error> {
    let text = string(value)?;
    match text {
        Value::Text(points) => Ok(Value::Text(crate::state::trim(&points).to_vec()).string()?),
        other => Ok(other.string()?),
    }
}
fn seconds(value: &Value, default: i64, allowed: &[u64], fallback: u64) -> u64 {
    let value = value
        .int()
        .ok()
        .and_then(|number| number.to_i64())
        .unwrap_or(default);
    u64::try_from(value)
        .ok()
        .filter(|value| allowed.contains(value))
        .unwrap_or_else(|| {
            u64::try_from(default)
                .ok()
                .filter(|value| allowed.contains(value))
                .unwrap_or(fallback)
        })
}
impl StartOptions {
    pub fn parse(value: &Value, config: &super::Config) -> Result<Self, Error> {
        Ok(Self {
            note: clipped(string(value.get("note"))?, 500),
            ttl: seconds(
                value.get("ttl_seconds"),
                config.default_ttl,
                &[900, 1800, 3600],
                1800,
            ),
            permission: if stripped(value.get("permission_mode"))? == "allow_all" {
                Permission::AllowAll
            } else {
                Permission::ApproveEach
            },
            command_timeout: seconds(
                value.get("command_timeout_seconds"),
                config.default_command_timeout,
                &[15, 30, 60, 120],
                30,
            ),
        })
    }
}
pub(super) fn now() -> f64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(time) => time.as_secs_f64(),
        Err(time) => -time.duration().as_secs_f64(),
    }
}
pub(super) fn typing(value: &Value) -> Result<String, Error> {
    let Value::Text(points) = string(value)? else {
        return Err(Error::Source("expected Python text".into()));
    };
    let mut output = String::new();
    let mut count = 0;
    for point in points {
        if let Some(character) = char::from_u32(point) {
            if Value::Text(vec![point]).repr()?.contains(character) {
                output.push(character);
                count += 1;
                if count == 160 {
                    break;
                }
            }
        }
    }
    Ok(output)
}
