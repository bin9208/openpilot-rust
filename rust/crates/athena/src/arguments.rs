use crate::rpc::Fault;
use openpilot_logmessaged::{JsonValue, JsonView};

pub fn signature(method: &str) -> Option<(&'static [&'static str], usize)> {
    Some(match method {
        "echo" => (&["s"], 1),
        "getMessage" => (&["service", "timeout"], 1),
        "listDataDirectory" => (&["prefix"], 0),
        "uploadFileToUrl" => (&["fn", "url", "headers"], 3),
        "uploadFilesToUrls" => (&["files_data"], 1),
        "cancelUpload" => (&["upload_id"], 1),
        "setRouteViewed" => (&["route"], 1),
        "startLocalProxy" => (&["remote_ws_uri", "local_port"], 2),
        "getVersion"
        | "listUploadQueue"
        | "getPublicKey"
        | "getSshAuthorizedKeys"
        | "getGithubUsername"
        | "getSimInfo"
        | "getNetworkType"
        | "getNetworkMetered"
        | "getNetworks"
        | "takeSnapshot" => (&[], 0),
        _ => return None,
    })
}
pub fn bind(method: &str, params: Option<JsonValue>) -> Result<Vec<JsonValue>, Fault> {
    let (names, required) = signature(method).ok_or_else(|| Fault::standard(-32601))?;
    let function = if method == "echo" { "<lambda>" } else { method };
    let mut values = vec![None; names.len()];
    if let Some(params) = params {
        match params.view() {
            JsonView::Array(args) => {
                if args.len() > names.len() {
                    let count = if names.len() == required {
                        names.len().to_string()
                    } else {
                        format!("from {required} to {}", names.len())
                    };
                    return Err(Fault::params(format!(
                        "{function}() takes {count} positional argument{} but {} {} given",
                        if names.len() == 1 { "" } else { "s" },
                        args.len(),
                        if args.len() == 1 { "was" } else { "were" }
                    )));
                }
                for (slot, value) in values.iter_mut().zip(args) {
                    *slot = Some(value);
                }
            }
            JsonView::Object(args) => {
                for (key, value) in args {
                    let name: String = key.iter().copied().filter_map(char::from_u32).collect();
                    let Some(index) = names.iter().position(|&field| field == name) else {
                        return Err(Fault::params(format!(
                            "{function}() got an unexpected keyword argument '{name}'"
                        )));
                    };
                    values[index] = Some(value);
                }
            }
            JsonView::Null => {}
            _ => return Err(Fault::standard(-32600)),
        }
    }
    let missing: Vec<_> = values
        .iter()
        .take(required)
        .enumerate()
        .filter(|(_, value)| value.is_none())
        .map(|(index, _)| format!("'{}'", names[index]))
        .collect();
    if !missing.is_empty() {
        let fields = match missing.as_slice() {
            [one] => one.clone(),
            [one, two] => format!("{one} and {two}"),
            many => format!(
                "{}, and {}",
                many[..many.len() - 1].join(", "),
                many[many.len() - 1]
            ),
        };
        return Err(Fault::params(format!(
            "{function}() missing {} required positional argument{}: {fields}",
            missing.len(),
            if missing.len() == 1 { "" } else { "s" }
        )));
    }
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            value.map_or_else(
                || {
                    if method == "getMessage" && index == 1 {
                        JsonValue::parse("1000").map_err(Fault::from)
                    } else {
                        Ok(JsonValue::text(""))
                    }
                },
                Ok,
            )
        })
        .collect()
}
