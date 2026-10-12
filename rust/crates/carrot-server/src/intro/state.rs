use super::Intro;
use crate::{Error, Value};
use std::fs;

impl Intro {
    fn path(&self) -> std::path::PathBuf {
        self.config.state.join("intro.json")
    }

    pub(super) fn read_locked(&self) -> Result<Value, Error> {
        let raw = fs::read_to_string(self.path())
            .ok()
            .and_then(|text| Value::parse(&text).ok())
            .filter(|value| matches!(value, Value::Object(_)))
            .unwrap_or_else(|| Value::Object(Vec::new()));
        let version = raw.get("version");
        let version = if version.truth() {
            version.clone()
        } else {
            Value::integer(1)
        };
        let reason = raw.get("reason");
        let reason = if reason.truth() {
            reason.py_string()?
        } else {
            Value::text("")
        };
        let completed_at = raw.get("completedAt");
        Ok(Value::object([
            ("version", Value::Integer(version.int()?)),
            ("completed", Value::Bool(raw.get("completed").truth())),
            (
                "completedAt",
                if completed_at.truth() {
                    completed_at.clone()
                } else {
                    Value::integer(0)
                },
            ),
            ("reason", reason),
        ]))
    }

    pub(super) fn mark_locked(&self, reason: &Value) -> Result<Value, Error> {
        let reason = if reason.truth() {
            reason.py_string()?
        } else {
            Value::text("user_finished")
        };
        let Value::Text(mut reason) = reason else {
            return Err(Error::Source("intro reason is not text".into()));
        };
        reason.truncate(64);
        let data = Value::object([
            ("version", Value::integer(1)),
            ("completed", Value::Bool(true)),
            ("completedAt", self.timestamp()),
            ("reason", Value::Text(reason)),
        ]);
        if let Err(error) = self.write_state(&data) {
            eprintln!("intro completion state: {error}");
        }
        Ok(data)
    }

    fn write_state(&self, data: &Value) -> Result<(), Error> {
        fs::create_dir_all(&self.config.state)?;
        let temporary = self.config.state.join("intro.json.tmp");
        crate::state_json::write_json(&temporary, data)?;
        fs::rename(temporary, self.path())?;
        Ok(())
    }

    pub(super) fn bootstrap_locked(&self, params: &crate::params::Backend) -> Result<Value, Error> {
        let state = self.read_locked()?;
        if state.get("completed").truth() {
            let reason = if state.get("reason").truth() {
                state.get("reason").clone()
            } else {
                Value::text("already_completed")
            };
            return Ok(Value::object([
                ("shouldShow", Value::Bool(false)),
                ("reason", reason),
            ]));
        }
        let existing = self.existing_reason(params)?;
        if !existing.is_empty() {
            self.mark_locked(&Value::text("existing_install"))?;
            return Ok(Value::object([
                ("shouldShow", Value::Bool(false)),
                ("reason", Value::text(&existing)),
            ]));
        }
        Ok(Value::object([
            ("shouldShow", Value::Bool(true)),
            ("reason", Value::text("fresh_install")),
        ]))
    }

    pub fn reset(
        &self,
        params: &crate::params::Backend,
    ) -> Result<(hyper::StatusCode, Value), Error> {
        let _guard = self
            .state_lock
            .lock()
            .map_err(|_| Error::Source("intro state lock poisoned".into()))?;
        let path = self.path();
        if path.exists() {
            if let Err(error) = fs::remove_file(&path) {
                let message = error.to_string();
                let message = message.split(" (os error").next().unwrap_or(&message);
                let error = format!(
                    "[Errno {}] {message}: {}",
                    error.raw_os_error().unwrap_or(0),
                    Value::text(&path.to_string_lossy()).repr()?
                );
                return Ok((
                    hyper::StatusCode::INTERNAL_SERVER_ERROR,
                    Value::object([("ok", Value::Bool(false)), ("error", Value::text(&error))]),
                ));
            }
        }
        let bootstrap = self.bootstrap_locked(params)?;
        Ok((
            hyper::StatusCode::OK,
            Value::object([
                ("ok", Value::Bool(true)),
                ("shouldShow", bootstrap.get("shouldShow").clone()),
                ("reason", bootstrap.get("reason").clone()),
            ]),
        ))
    }
}
