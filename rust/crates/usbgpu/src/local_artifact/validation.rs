use super::source::{fingerprint, suffix};
use crate::{
    model_delivery::failure::{transient, Kind},
    Error,
};
use std::{fs, io::Write, path::Path};

pub fn identity(model: &Path) -> Result<serde_json::Value, Error> {
    let (bytes, digest) = fingerprint(model)?;
    Ok(serde_json::json!({"kind":"run-policy","pickle_bytes":bytes,"pickle_sha256":digest}))
}
pub fn cached(model: &Path, key: &str) -> bool {
    fs::read(suffix(model, ".native_boot_validation.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| value == serde_json::json!({"key":key}))
}
pub fn save(model: &Path, key: &str) -> Result<(), Error> {
    write(
        model,
        ".native_boot_validation.json",
        &serde_json::json!({"key":key}),
    )
}
fn write(model: &Path, extension: &str, value: &serde_json::Value) -> Result<(), Error> {
    let root = model
        .parent()
        .ok_or(Error::Contract("local model directory missing"))?;
    let mut stage = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer(&mut stage, value)?;
    stage.flush()?;
    stage.as_file().sync_all()?;
    stage
        .persist(suffix(model, extension))
        .map_err(|error| error.error)?;
    Ok(())
}
pub fn record(
    model: &Path,
    detail: &str,
    kind: Kind,
    phase: &str,
    time: f64,
) -> Result<bool, Error> {
    match fs::remove_file(suffix(model, ".native_boot_validation.json")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let (_, digest) = fingerprint(model)?;
    let rejected = !transient(detail, kind);
    let start = detail
        .char_indices()
        .rev()
        .nth(16383)
        .map_or(0, |(index, _)| index);
    write(
        model,
        ".native_last_failure.json",
        &serde_json::json!({"time":time,"phase":phase,"rejected":rejected,
        "pickle_sha256":digest,"error":&detail[start..]}),
    )?;
    if rejected {
        fs::write(suffix(model, ".native_rejected"), digest)?;
    }
    Ok(rejected)
}
