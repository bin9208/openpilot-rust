use super::key::Key;
use crate::{Error, Value};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub(super) struct Keys {
    pub path: PathBuf,
    cached: Option<(i64, i64, Key)>,
}
impl Keys {
    pub fn new(path: PathBuf) -> Self {
        Self { path, cached: None }
    }
    pub fn get(&mut self) -> Key {
        let Ok(metadata) = fs::metadata(&self.path) else {
            self.cached = None;
            return Key::default();
        };
        if let Some((seconds, nanos, key)) = &self.cached {
            if *seconds == metadata.mtime() && *nanos == metadata.mtime_nsec() {
                return key.clone();
            }
        }
        let data = read(&self.path);
        let key = Key::from_value(data.get("stream_key")).unwrap_or_default();
        self.cached = Some((metadata.mtime(), metadata.mtime_nsec(), key.clone()));
        key
    }
    pub fn set(&mut self, value: &Value) -> Result<(), Error> {
        let key = Key::extract(value)?;
        if !key.configured() {
            return Err(Error::Source("stream key is required".into()));
        }
        write(
            &self.path,
            &Value::object([
                ("stream_key", key.value()),
                ("updated_at", Value::Float(super::clock::now().wall)),
            ]),
            Some(0o600),
        )
    }
    pub fn clear(&mut self) -> Result<(), Error> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
pub(super) fn read(path: &Path) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|value| Value::parse(&value).ok())
        .filter(|value| matches!(value, Value::Object(_)))
        .unwrap_or_else(|| Value::object([]))
}
pub(super) fn write(path: &Path, value: &Value, mode: Option<u32>) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!(
        "{}.tmp",
        path.extension().unwrap_or_default().to_string_lossy()
    ));
    fs::write(&temp, value.encode()?)?;
    if let Some(mode) = mode {
        let _mode = fs::set_permissions(&temp, fs::Permissions::from_mode(mode));
    }
    fs::rename(temp, path)?;
    if let Some(mode) = mode {
        let _mode = fs::set_permissions(path, fs::Permissions::from_mode(mode));
    }
    Ok(())
}
