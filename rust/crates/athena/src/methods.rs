use crate::{
    ipc, logging, policy, proxy,
    rpc::Fault,
    state::{self, Shared, Stop},
};
use openpilot_logging::producer::Logger;
use openpilot_logmessaged::{JsonValue, JsonView};
use serde_json::{json, Value};
use std::{fs, path::Path, sync::Arc};

pub fn call(
    shared: &Arc<Shared>,
    stop: &Stop,
    logger: &mut Logger,
    method: &str,
    args: Vec<JsonValue>,
) -> Result<JsonValue, Fault> {
    match method {
        "echo" => Ok(args[0].clone()),
        "getMessage" => {
            let service = args[0]
                .to_utf8()
                .ok_or_else(|| Fault::server("Exception", "invalid service"))?;
            Ok(ipc::message(&service, integer(&args[1])?, stop)?)
        }
        "getVersion" => {
            let metadata = openpilot_runtime_version::get_build_metadata(&shared.config.basedir)
                .map_err(crate::Error::from)?;
            JsonValue::parse(&format!(
                "{{\"version\":{},\"remote\":{},\"branch\":{},\"commit\":{}}}",
                metadata
                    .openpilot
                    .version
                    .to_json()
                    .map_err(crate::Error::from)?,
                metadata
                    .openpilot
                    .git_normalized_origin()
                    .map_err(crate::Error::from)?
                    .to_json()
                    .map_err(crate::Error::from)?,
                metadata.channel.to_json().map_err(crate::Error::from)?,
                metadata
                    .openpilot
                    .git_commit
                    .to_json()
                    .map_err(crate::Error::from)?
            ))
            .map_err(Fault::from)
        }
        "listDataDirectory" => encode(
            scan(
                &shared.config.log_root,
                &shared.config.log_root,
                &text(&args[0])?,
            )?
            .into(),
        ),
        "uploadFilesToUrls" | "uploadFileToUrl" => {
            let files = if method == "uploadFilesToUrls" {
                let Value::Array(values) = value(&args[0])? else {
                    return Err(Fault::server("TypeError", "files_data is not iterable"));
                };
                values
            } else {
                vec![
                    json!({"fn":value(&args[0])?,"url":value(&args[1])?,"headers":value(&args[2])?}),
                ]
            };
            let result =
                shared
                    .uploads()?
                    .enqueue(&shared.config.log_root, &files, state::now_ms()?)?;
            cache(shared, logger);
            shared.available.notify_all();
            encode(result)
        }
        "listUploadQueue" => {
            encode(serde_json::to_value(shared.uploads()?.list()).map_err(crate::Error::from)?)
        }
        "cancelUpload" => {
            let ids = match args[0].view() {
                JsonView::Array(values) => values,
                _ => vec![args[0].clone()],
            };
            let ids = ids
                .iter()
                .map(|value| match value.view() {
                    JsonView::Null => Ok(None),
                    JsonView::Text(_) => Ok(Some(text(value)?)),
                    _ => Err(Fault::server("TypeError", "unhashable upload id")),
                })
                .collect::<Result<Vec<_>, Fault>>()?;
            encode(shared.uploads()?.cancel(&ids))
        }
        "setRouteViewed" => {
            let previous = shared.text("AthenadRecentlyViewedRoutes", logger)?;
            let routes = policy::viewed_routes(previous.as_deref(), &text(&args[0])?);
            shared
                .params
                .put("AthenadRecentlyViewedRoutes", routes.as_bytes())
                .map_err(crate::Error::from)?;
            encode(json!({"success":1}))
        }
        "startLocalProxy" => {
            proxy::start(
                Arc::clone(shared),
                stop.clone(),
                &text(&args[0])?,
                integer(&args[1])?,
                logger,
            )?;
            encode(json!({"success":1}))
        }
        "getPublicKey" => encode(
            openpilot_registration::get_key_pair(&shared.config.persist_root)
                .map_err(crate::Error::from)?
                .map_or(Value::Null, |pair| pair.public.into()),
        ),
        "getSshAuthorizedKeys" | "getGithubUsername" => {
            let key = if method == "getSshAuthorizedKeys" {
                "GithubSshKeys"
            } else {
                "GithubUsername"
            };
            Ok(JsonValue::text(
                &shared.text(key, logger)?.unwrap_or_default(),
            ))
        }
        "getSimInfo" => Ok(openpilot_hardware_info::for_runtime()
            .get_sim_info()
            .map_err(crate::Error::from)?),
        "getNetworkType" => encode(json!(
            openpilot_hardware_info::for_runtime()
                .get_network_type()
                .map_err(crate::Error::from)?
                .0
        )),
        "getNetworkMetered" => {
            let hardware = openpilot_hardware_info::for_runtime();
            encode(
                hardware
                    .get_network_metered(hardware.get_network_type().map_err(crate::Error::from)?)
                    .map_err(crate::Error::from)?
                    .into(),
            )
        }
        "getNetworks" => match openpilot_hardware_info::for_runtime()
            .get_networks()
            .map_err(crate::Error::from)?
        {
            Some(networks) => Ok(networks.to_json().map_err(crate::Error::from)?),
            None => encode(Value::Null),
        },
        "takeSnapshot" => encode(crate::snapshot::take(shared, stop)?),
        _ => Err(Fault::standard(-32601)),
    }
}
fn scan(root: &Path, path: &Path, prefix: &str) -> Result<Vec<String>, Fault> {
    let mut files = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| Fault::io(error, path))? {
        let entry = entry.map_err(|error| Fault::io(error, path))?;
        let rel = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| Fault::server("Exception", "relative path"))?
            .to_string_lossy()
            .into_owned();
        if entry
            .file_type()
            .map_err(|error| Fault::io(error, &entry.path()))?
            .is_dir()
        {
            let directory = format!("{rel}/");
            if directory.starts_with(prefix) || prefix.starts_with(&directory) {
                files.extend(scan(root, &entry.path(), prefix)?);
            }
        } else if rel.starts_with(prefix) {
            files.push(rel);
        }
    }
    Ok(files)
}
pub fn cache(shared: &Shared, logger: &mut Logger) {
    if let Err(error) = shared.cache() {
        logging::failure(logger, "athena.UploadQueueCache.cache.exception", &error);
    }
}
pub fn encode(value: Value) -> Result<JsonValue, Fault> {
    JsonValue::parse(&value.to_string()).map_err(Fault::from)
}
fn value(value: &JsonValue) -> Result<Value, Fault> {
    serde_json::from_str(&value.to_json().map_err(crate::Error::from)?)
        .map_err(crate::Error::from)
        .map_err(Fault::from)
}
fn text(value: &JsonValue) -> Result<String, Fault> {
    value
        .to_utf8()
        .ok_or_else(|| Fault::server("TypeError", "expected text"))
}
fn integer(value: &JsonValue) -> Result<i64, Fault> {
    match value.view() {
        JsonView::Integer(text) => text.parse().map_err(|_| {
            Fault::server("OverflowError", "Python int too large to convert to C long")
        }),
        JsonView::Bool(value) => Ok(i64::from(value)),
        _ => Err(Fault::server("TypeError", "an integer is required")),
    }
}
