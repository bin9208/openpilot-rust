use super::{Failure, Service, Value};
use crate::json_fields::set;
use openpilot_bluetooth::{Address, Config};
use std::path::Path;

fn read(path: &Path) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| Value::parse(&text).ok())
        .unwrap_or_else(|| Value::Object(Vec::new()))
}

impl Service {
    pub(super) fn runtime(&self) -> Result<Value, Failure> {
        let mut state = read(&self.runtime.join("status.json"));
        if !matches!(state, Value::Object(_)) {
            state = Value::Object(Vec::new());
        }
        let default_stamp = Value::integer(0);
        let stamp = if state.has("time") {
            state.get("time")
        } else {
            &default_stamp
        };
        let age = if matches!(stamp, Value::Integer(_) | Value::Float(_) | Value::Bool(_)) {
            Some(self.now() - stamp.float()?)
        } else {
            None
        };
        let alive =
            age.is_some_and(|age| (0. ..2.).contains(&age)) && !state.get("stopped").truth();
        let stationary = alive && state.get("stationary").truth();
        set(&mut state, "alive", Value::Bool(alive))?;
        set(&mut state, "stationary", Value::Bool(stationary))?;
        Ok(state)
    }

    pub(super) fn settings(&self) -> Config {
        std::fs::read_to_string(&self.config)
            .ok()
            .and_then(|text| Config::parse(&text).ok())
            .unwrap_or_default()
    }

    pub(super) fn cancel_pending(&self, address: &Address) -> Result<(), Failure> {
        let path = self.runtime.join("cancelled.json");
        let mut cancelled = read(&path);
        if !matches!(cancelled, Value::Object(_)) {
            cancelled = Value::Object(Vec::new());
        }
        set(&mut cancelled, address.as_str(), Value::Float(self.now()))?;
        atomic(&path, &cancelled)
    }
}

pub(super) fn atomic(path: &Path, value: &Value) -> Result<(), Failure> {
    Ok(openpilot_bluetooth::atomic_value(
        path,
        &openpilot_logmessaged::JsonValue::parse(&value.encode()?)?,
    )?)
}
