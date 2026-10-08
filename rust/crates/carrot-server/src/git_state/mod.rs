mod pull_output;

use crate::{Error, Value};
pub use pull_output::did_pull_update;
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

pub const HISTORY_LIMIT: usize = 20;

pub struct Store {
    directory: PathBuf,
    path: PathBuf,
}

pub struct Time {
    pub seconds: Value,
    pub nanoseconds: Value,
}

impl Store {
    pub fn new(directory: PathBuf) -> Self {
        let path = directory.join("git.json");
        Self { directory, path }
    }

    pub fn read(&self) -> Value {
        crate::state::read(&self.path)
    }

    pub fn write(&self, data: &Value) -> bool {
        let write = || -> Result<(), Error> {
            fs::create_dir_all(&self.directory)?;
            let mut temporary = self.path.as_os_str().to_os_string();
            temporary.push(".tmp");
            let temporary = PathBuf::from(temporary);
            let mut file = File::create(&temporary)?;
            let encoded = crate::state_json::compact_encoded(&data.encode()?);
            file.write_all(encoded.as_bytes())?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &self.path)?;
            Ok(())
        };
        write().is_ok()
    }

    pub fn custom_meta(&self, name: &str) -> Option<Value> {
        if name != "GitPullTime" {
            return None;
        }
        let state = self.read();
        let value = state.get("git_pull_time");
        if matches!(value, Value::Null) {
            return None;
        }
        crate::param_changes::text::stripped(value, false).ok()
    }

    pub fn write_pull_time(&self, timestamp: &Value, clock: &Time) -> Result<(), Error> {
        let timestamp = if matches!(timestamp, Value::Null) {
            &clock.seconds
        } else {
            timestamp
        };
        let value = Value::Integer(timestamp.int()?);
        let mut data = self.read();
        crate::json_fields::set(&mut data, "git_pull_time", value)?;
        crate::json_fields::set(&mut data, "git_pull_ok", Value::Bool(true))?;
        self.write(&data);
        Ok(())
    }

    pub fn auto_update(&self) -> Value {
        match self.read().get("auto_update") {
            value @ Value::Object(_) => value.clone(),
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::Float(_)
            | Value::Text(_)
            | Value::Array(_) => Value::Object(Vec::new()),
        }
    }

    pub fn write_event(
        &self,
        status: &Value,
        fields: &Value,
        clock: &Time,
    ) -> Result<Value, Error> {
        let fields = crate::json_fields::fields(fields)?;
        if fields
            .iter()
            .any(|(key, _)| key.iter().copied().eq("status".chars().map(u32::from)))
        {
            return Err(openpilot_carrot_navi::Error::typed(
                "TypeError",
                "write_auto_update_event() got multiple values for argument 'status'".into(),
            )
            .into());
        }
        let mut data = self.read();
        let mut state = match data.get("auto_update") {
            value @ Value::Object(_) => value.clone(),
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::Float(_)
            | Value::Text(_)
            | Value::Array(_) => Value::Object(Vec::new()),
        };
        let event_id = clock.nanoseconds.py_string()?;
        let now = Value::Integer(clock.seconds.int()?);
        for (key, value) in fields {
            let Value::Object(state) = &mut state else {
                return Err(Error::Source("Git update state is not an object".into()));
            };
            crate::json_fields::insert(state, key.clone(), value.clone());
        }
        let status = if status.truth() {
            status.py_string()?
        } else {
            Value::text("unknown")
        };
        crate::json_fields::set(&mut state, "status", status.clone())?;
        crate::json_fields::set(&mut state, "event_id", event_id.clone())?;
        crate::json_fields::set(&mut state, "updated_at", now.clone())?;
        crate::json_fields::set(&mut data, "auto_update", state.clone())?;
        let mut history = match data.get("auto_update_history") {
            Value::Array(items) => items.clone(),
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::Float(_)
            | Value::Text(_)
            | Value::Object(_) => Vec::new(),
        };
        let mut event = Value::object([
            ("status", status),
            ("event_id", event_id),
            ("updated_at", now),
        ]);
        for name in [
            "attempted_at",
            "old_head",
            "new_head",
            "target_head",
            "reset_rc",
            "pull_rc",
            "error_code",
            "error",
            "reboot_mode",
            "reboot_requested_head",
        ] {
            if let Some((_, value)) = fields
                .iter()
                .find(|(key, _)| key.iter().copied().eq(name.chars().map(u32::from)))
            {
                crate::json_fields::set(&mut event, name, value.clone())?;
            }
        }
        history.push(event);
        let excess = history.len().saturating_sub(HISTORY_LIMIT);
        history.drain(..excess);
        crate::json_fields::set(&mut data, "auto_update_history", Value::Array(history))?;
        Ok(if self.write(&data) {
            state
        } else {
            Value::Object(Vec::new())
        })
    }
}
