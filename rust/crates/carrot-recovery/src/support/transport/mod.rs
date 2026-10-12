mod binary;
mod tunnel;
mod webhook;
pub use tunnel::Tunnel;

use crate::{config::Config, cwp::read_param, Error, Value};
use openpilot_cweb_push::helpers;
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub fn notify(config: &Config, payload: &Value) -> Value {
    match webhook::send(config, payload) {
        Ok(result) => result,
        Err(error) => crate::upload::network::failure(error),
    }
}

pub fn metadata(config: &Config) -> Result<Value, Error> {
    let git = |args: &[&str]| -> Result<String, Error> {
        let cwd = if config.repository.join("openpilot").is_dir() {
            config.repository.join("openpilot")
        } else {
            config.repository.clone()
        };
        let command = CapturedCommand {
            launcher: config.launcher.clone(),
            cwd,
            argv: args.iter().map(Into::into).collect(),
        };
        let output = command
            .capture(Duration::from_secs(6), None)
            .map_err(|error| Error::Source(error.to_string()))?;
        let mut bytes = output.stdout;
        bytes.extend_from_slice(&output.stderr);
        let text = String::from_utf8_lossy(&bytes)
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let text = helpers::strip(&text);
        Ok(if text.is_empty() {
            "unknown".into()
        } else {
            text.into()
        })
    };
    let serial = ["HardwareSerial", "DeviceSerial", "Serial"]
        .into_iter()
        .map(|key| read_param(config, key, ""))
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".into());
    Ok(Value::object([
        (
            "carName",
            Value::text(&read_param(config, "CarName", "none")),
        ),
        (
            "dongleId",
            Value::text(&read_param(config, "DongleId", "unknown")),
        ),
        ("serial", Value::text(&serial)),
        (
            "branch",
            Value::text(&git(&["git", "branch", "--show-current"])?),
        ),
        (
            "commit",
            Value::text(&git(&["git", "rev-parse", "--short", "HEAD"])?),
        ),
        (
            "host",
            Value::text(&rustix::system::uname().nodename().to_string_lossy()),
        ),
    ]))
}

#[cfg(test)]
mod tests;
