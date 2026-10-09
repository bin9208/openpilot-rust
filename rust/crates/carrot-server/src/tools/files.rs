use super::{
    context::{Context, Reply},
    runner::Failure,
};
use crate::{Error, Value};
use std::{fs, path::Path};

pub(super) fn delete(context: &Context, logs: bool) -> Result<Reply, Failure> {
    context.progress(if logs { "delete logs" } else { "delete videos" }, 1, 1)?;
    let path = if logs {
        &context.config.paths.logs
    } else {
        &context.config.paths.videos
    };
    let mut deleted = 0;
    if path.is_dir() {
        let entries = fs::read_dir(path).map_err(|error| crate::state::io_error(error, path))?;
        for entry in entries {
            let entry = entry.map_err(Error::from)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !logs && name.starts_with('.') {
                continue;
            }
            let full = entry.path();
            let result = if !logs {
                fs::remove_file(&full)
            } else {
                let metadata = fs::symlink_metadata(&full)
                    .map_err(|error| crate::state::io_error(error, &full))?;
                if metadata.is_file() || metadata.file_type().is_symlink() {
                    fs::remove_file(&full)
                } else if metadata.is_dir() {
                    fs::remove_dir_all(&full)
                } else {
                    continue;
                }
            };
            match result {
                Ok(()) => {
                    deleted += 1;
                    context.append(&format!("deleted: {name}"))?;
                }
                Err(error) => {
                    if context.streaming() {
                        context.append(&format!(
                            "delete error: {}",
                            crate::state::io_error(error, &full)
                        ))?;
                    }
                }
            }
        }
    }
    Ok(Reply::ok(Value::object([
        ("ok", Value::Bool(true)),
        (
            "out",
            Value::text(&format!(
                "deleted {}: {deleted}",
                if logs { "entries" } else { "files" }
            )),
        ),
    ])))
}
pub(super) fn backup(context: &Context) -> Result<Reply, Failure> {
    let Some(params) = crate::system::fresh::reopen(context.config.params.as_ref())? else {
        return Ok(Reply::error(500, "Params/ParamKeyType not available"));
    };
    context.progress("backup settings", 1, 1)?;
    let backend = crate::params::Backend::native(
        params,
        context
            .config
            .paths
            .history
            .parent()
            .unwrap_or(Path::new("."))
            .into(),
    );
    let values = backend.backup_values()?;
    let count = match &values {
        Value::Object(fields) => fields.len(),
        _ => 0,
    };
    let path = &context.config.paths.backup;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| crate::state::io_error(error, parent))?;
    }
    let Value::Object(fields) = &values else {
        return Err(Error::Source("backup values must be a mapping".into()).into());
    };
    let mut encoded = String::from("{\n");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            encoded.push_str(",\n");
        }
        encoded.push_str("  ");
        encoded.push_str(&crate::state_json::utf8_text(key, "")?);
        encoded.push_str(": ");
        match value {
            Value::Text(points) => encoded.push_str(&crate::state_json::utf8_text(points, "")?),
            value => encoded.push_str(&value.encode().map_err(Error::from)?),
        }
    }
    encoded.push_str("\n}");
    fs::write(path, encoded).map_err(|error| crate::state::io_error(error, path))?;
    Ok(Reply::ok(Value::object([
        ("ok", Value::Bool(true)),
        ("out", Value::text(&format!("backup saved ({count} keys)"))),
        ("file", Value::text("/download/params_backup.json")),
    ])))
}
pub(super) fn send_tmux(context: &Context) -> Result<Reply, Failure> {
    context.progress("send tmux", 1, 1)?;
    let params = crate::system::fresh::reopen(context.config.params.as_ref())?
        .ok_or_else(|| Error::Source("Params unavailable".into()))?;
    let state = context
        .config
        .paths
        .history
        .parent()
        .unwrap_or(Path::new("."));
    // Source's local Params destructor joins its queued write; the Cython put
    // discards filesystem status. Reuse that already-converted put contract.
    let mut backend = crate::params::Backend::native(params, state.into());
    backend.put("CarrotException", &Value::text("tmux_send"), None)?;
    Ok(Reply::ok(Value::object([
        ("ok", Value::Bool(true)),
        ("out", Value::text("tmux send triggered")),
    ])))
}
pub(super) fn calibration(context: &Context) -> Result<Reply, Failure> {
    context.progress("reset calibration", 1, 1)?;
    let mut messages = Vec::new();
    for path in &context.config.paths.calibration {
        let message = match fs::remove_file(path) {
            Ok(()) => Some(format!("removed {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => Some(format!(
                "error removing {}: {}",
                path.display(),
                crate::state::io_error(error, path)
            )),
        };
        if let Some(message) = message {
            context.append(&message)?;
            messages.push(message);
        }
    }
    let output = if context.streaming() || messages.is_empty() {
        "calibration reset".into()
    } else {
        messages.join("\n")
    };
    Ok(Reply::ok(Value::object([
        ("ok", Value::Bool(true)),
        ("out", Value::text(&output)),
    ])))
}
