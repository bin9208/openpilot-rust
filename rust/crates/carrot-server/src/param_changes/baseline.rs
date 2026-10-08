use super::{json::utf8, text, History};
use crate::{Error, Value};
use std::fs;

impl History {
    pub fn read_baseline(&self) -> Result<Option<Value>, Error> {
        let raw = match fs::read_to_string(&self.paths.baseline) {
            Ok(text) => match Value::parse(&text) {
                Ok(value @ Value::Object(_)) => value,
                Ok(_) | Err(_) => return Ok(None),
            },
            Err(_) => return Ok(None),
        };
        let fingerprint = text::string(raw.get("fingerprint"), true)?;
        if !fingerprint.truth() {
            return Ok(None);
        }
        let ts = if raw.get("ts").truth() {
            raw.get("ts").int()?
        } else {
            0.into()
        };
        Ok(Some(Value::object([
            ("fingerprint", fingerprint),
            ("ts", Value::Integer(ts)),
        ])))
    }
    pub fn write_baseline(&self, fingerprint: &Value) -> Result<Value, Error> {
        let record = Value::object([
            ("fingerprint", fingerprint.py_string()?),
            ("ts", self.timestamp()?),
        ]);
        let parent = self
            .paths
            .baseline
            .parent()
            .ok_or_else(|| Error::Source("invalid baseline directory".into()))?;
        fs::create_dir_all(parent)?;
        let temporary = self.paths.baseline.with_file_name(format!(
            "{}.tmp",
            self.paths
                .baseline
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        fs::write(&temporary, format!("{}\n", utf8(&record)?))?;
        fs::rename(temporary, &self.paths.baseline)?;
        Ok(record)
    }
}
