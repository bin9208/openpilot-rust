use crate::{Error, Value};
use std::{fs, path::Path};

pub(crate) fn read(path: &Path) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|source| Value::parse(&source).ok())
        .filter(|value| matches!(value, Value::Object(_)))
        .unwrap_or_else(|| Value::Object(Vec::new()))
}

pub(crate) fn trim(points: &[u32]) -> &[u32] {
    let whitespace = |point: &u32| {
        char::from_u32(*point)
            .is_some_and(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
    };
    let start = points
        .iter()
        .position(|point| !whitespace(point))
        .unwrap_or(points.len());
    let end = points
        .iter()
        .rposition(|point| !whitespace(point))
        .map_or(start, |index| index + 1);
    &points[start..end]
}

fn io_error(error: std::io::Error, path: &Path) -> Error {
    let message = error.to_string();
    let message = message.split(" (os error").next().unwrap_or(&message);
    Error::Source(format!(
        "[Errno {}] {message}: {}",
        error.raw_os_error().unwrap_or(0),
        Value::text(&path.to_string_lossy())
            .repr()
            .unwrap_or_default()
    ))
}

pub(crate) fn write(path: &Path, value: &Value) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Source("invalid state directory".into()))?;
    fs::create_dir_all(parent).map_err(|error| io_error(error, parent))?;
    let temporary = path.with_file_name(format!(
        "{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    crate::state_json::write_json(&temporary, value).map_err(|error| match error {
        Error::Io(error) => io_error(error, &temporary),
        Error::Source(_)
        | Error::UnknownCharset(_)
        | Error::Request(_)
        | Error::Json(_)
        | Error::Params(_) => error,
    })?;
    fs::rename(&temporary, path).map_err(|error| io_error(error, &temporary))?;
    Ok(())
}
