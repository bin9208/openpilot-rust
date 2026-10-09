use super::{config::Config, process, storage};
use crate::{Error, Value};
use openpilot_msgq::{VisionClient, VisionStream};
use std::time::Duration;

pub(super) fn streams() -> Vec<i32> {
    let mut streams: Vec<_> = VisionClient::available_streams("camerad")
        .unwrap_or_default()
        .into_iter()
        .map(|stream| match stream {
            VisionStream::Road => 0,
            VisionStream::Driver => 1,
            VisionStream::WideRoad => 2,
            VisionStream::Map => 3,
        })
        .collect();
    streams.sort_unstable();
    streams
}
pub(super) fn boolean(config: &Config, name: &str) -> bool {
    config
        .params
        .get(name)
        .ok()
        .flatten()
        .is_some_and(|value| value == b"1")
}
pub(super) fn param_integer(config: &Config, name: &str) -> i32 {
    let value = config.params.get(name).ok().flatten().unwrap_or_default();
    match openpilot_beepd::integer(&value) {
        Ok(value) => value,
        Err(error) => crate::param_native::fatal(name, &error),
    }
}
pub(super) fn youtube(config: &Config) -> Value {
    let fetched = (|| -> Result<Value, Error> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(500)))
            .build()
            .into();
        let bytes = agent
            .get(&config.status_url)
            .call()
            .map_err(|error| Error::Source(error.to_string()))?
            .body_mut()
            .read_to_vec()
            .map_err(|error| Error::Source(error.to_string()))?;
        let text = String::from_utf8(bytes).map_err(|error| Error::Source(error.to_string()))?;
        let value = Value::parse(&text)?;
        if !matches!(value, Value::Object(_)) {
            return Err(Error::Source("test status is not an object".into()));
        }
        Ok(value)
    })();
    fetched.unwrap_or_else(|_| storage::json(&config.paths.live_state))
}
pub fn get(config: &Config) -> Result<Value, Error> {
    let mut state = storage::json(&config.paths.state);
    let pid = i32::try_from(storage::integer(state.get("runner_pid"))?)
        .map_err(|_| Error::Source("test pid out of range".into()))?;
    let alive = process::alive(pid, &config.runner.pattern);
    let status = storage::text(state.get("status"));
    storage::set(
        &mut state,
        "status",
        Value::text(if alive || status == "error" {
            if status.is_empty() {
                "stopped"
            } else {
                &status
            }
        } else {
            "stopped"
        }),
    );
    storage::set(&mut state, "runner_pid", Value::integer(pid));
    storage::set(&mut state, "runner_alive", Value::Bool(alive));
    let quality = i32::try_from(storage::integer(state.get("quality"))?).unwrap_or(0);
    let children = state.get("children").clone();
    let mut rows = Vec::new();
    for (name, spec) in config.child_specs(quality) {
        let pid = i32::try_from(storage::integer(children.get(&name))?)
            .map_err(|_| Error::Source("test child pid out of range".into()))?;
        rows.push((
            name.chars().map(u32::from).collect(),
            Value::object([
                ("pid", Value::integer(pid)),
                ("alive", Value::Bool(process::alive(pid, &spec.pattern))),
            ]),
        ));
    }
    storage::set(&mut state, "children", Value::Object(rows));
    storage::set(
        &mut state,
        "vipc_streams",
        Value::Array(streams().into_iter().map(Value::integer).collect()),
    );
    storage::set(&mut state, "youtube", youtube(config));
    storage::set(
        &mut state,
        "device",
        Value::object([
            ("is_offroad", Value::Bool(boolean(config, "IsOffroad"))),
            ("is_onroad", Value::Bool(boolean(config, "IsOnroad"))),
            (
                "live_enabled",
                Value::Bool(param_integer(config, "CarrotYouTubeLive") > 0),
            ),
            (
                "quality",
                Value::integer(param_integer(config, "CarrotYouTubeQuality")),
            ),
            (
                "timestamp_enabled",
                Value::Bool(param_integer(config, "CarrotYouTubeTimestamp") > 0),
            ),
        ]),
    );
    storage::set(
        &mut state,
        "log_path",
        Value::text(&config.paths.log.to_string_lossy()),
    );
    Ok(state)
}
