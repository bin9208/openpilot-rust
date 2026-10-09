use super::{paths, Failure};
use crate::{Error, Value};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

pub fn normalize_recent_segment(value: &Value) -> Result<Value, Failure> {
    let value = crate::param_changes::text::stripped(value, true)?;
    match paths::safe_segment(&value) {
        Ok(value) => Ok(value),
        Err(Failure::Http { .. }) => Ok(Value::text("")),
        Err(error @ (Failure::InvalidRecent | Failure::Runtime(_))) => Err(error),
    }
}
pub struct ReadState {
    path: PathBuf,
    lock: Mutex<()>,
}
impl ReadState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }
    pub fn read(&self) -> Result<Value, Failure> {
        let state = crate::state::read(&self.path);
        Ok(Value::object([(
            "recentSegment",
            normalize_recent_segment(state.get("recentSegment"))?,
        )]))
    }
    pub fn write(&self, value: &Value, now: i64) -> Result<Value, Failure> {
        let value = normalize_recent_segment(value)?;
        if !value.truth() {
            return Err(Failure::InvalidRecent);
        }
        let directory = self
            .path
            .parent()
            .ok_or_else(|| Error::Source("invalid state directory".into()))?;
        let mut temporary = self.path.as_os_str().to_os_string();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        let _lock = self
            .lock
            .lock()
            .map_err(|_| Error::Source("dashcam state lock poisoned".into()))?;
        fs::create_dir_all(directory).map_err(|error| crate::state::io_error(error, directory))?;
        let mut file =
            File::create(&temporary).map_err(|error| crate::state::io_error(error, &temporary))?;
        let mut write = || -> Result<(), Error> {
            file.write_all(b"{\"version\":1,\"recentSegment\":")?;
            let encoded = crate::param_qr::compact_utf8(&value).map_err(|error| match error {
                Error::Source(message) => Error::Json(openpilot_carrot_navi::Error::typed(
                    "UnicodeEncodeError",
                    message,
                )),
                other @ (Error::Io(_)
                | Error::UnknownCharset(_)
                | Error::Request(_)
                | Error::Json(_)
                | Error::Params(_)) => other,
            })?;
            file.write_all(&encoded)?;
            file.write_all(format!(",\"updatedAt\":{now}}}\n").as_bytes())?;
            Ok(())
        };
        write().map_err(|error| match error {
            Error::Io(error) => crate::state::io_error(error, &temporary),
            other @ (Error::Source(_)
            | Error::UnknownCharset(_)
            | Error::Request(_)
            | Error::Json(_)
            | Error::Params(_)) => other,
        })?;
        drop(file);
        fs::rename(&temporary, &self.path).map_err(|error| {
            let first = crate::state::io_error(error, &temporary);
            let second = paths::path_value(&self.path).repr().unwrap_or_default();
            Error::Source(format!("{first} -> {second}"))
        })?;
        Ok(Value::object([("recentSegment", value)]))
    }
}
