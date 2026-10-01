use serde::Serialize;
use std::{fs, io::Write, os::unix::fs::PermissionsExt, path::Path};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    PythonJson(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Encoding(#[from] std::fmt::Error),
    #[error("runtime file needs a parent directory and filename")]
    Path,
    #[error("Bluetooth command sequence exhausted")]
    Sequence,
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    atomic(path, || {
        let json = serde_json::to_string(value)?;
        Ok(openpilot_logmessaged::JsonValue::parse(&json)?.to_json_utf8()?)
    })
}

pub fn atomic_value(path: &Path, value: &openpilot_logmessaged::JsonValue) -> Result<(), Error> {
    atomic(path, || Ok(value.to_json_utf8()?))
}

fn atomic(path: &Path, encode: impl FnOnce() -> Result<String, Error>) -> Result<(), Error> {
    let parent = path.parent().ok_or(Error::Path)?;
    let filename = path.file_name().ok_or(Error::Path)?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(filename)
        .suffix(".tmp")
        .tempfile_in(parent)?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    temporary.write_all(encode()?.as_bytes())?;
    temporary.flush()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}
