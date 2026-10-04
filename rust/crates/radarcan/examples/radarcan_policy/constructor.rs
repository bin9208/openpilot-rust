use super::{decoder, decoder_settings};
use openpilot_radarcan::{
    databases::Databases,
    decoder::{Config, Interface, Kind},
    Error,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Request {
    #[serde(flatten)]
    config: Config,
    constructor_ns: u64,
    #[serde(default)]
    params: Option<std::collections::BTreeMap<String, String>>,
}

pub fn trace(
    request: Value,
    databases: &mut Databases,
    stdout: &mut String,
) -> Result<Value, Error> {
    let request: Request = serde_json::from_value(request)?;
    let mut settings = decoder_settings::Observed {
        values: request.params,
        ..Default::default()
    };
    let constructed = Interface::with_settings(
        request.config,
        &mut openpilot_radarcan::decoder::hyundai::Environment {
            databases,
            clock: &mut || request.constructor_ns,
            emit: &mut |line: &str| stdout.push_str(line),
            settings: &mut settings,
        },
    );
    match constructed {
        Ok(mut state) => {
            let fallback = matches!(state.kind, Kind::Fallback);
            let (mut snapshot, warnings) = decoder::snapshot(&mut state)?;
            if fallback {
                snapshot["backend"] = json!({});
            }
            Ok(
                json!({"outcome":"ok","state":snapshot,"warnings":warnings,"parameter_reads":settings.reads}),
            )
        }
        Err(error) => Ok(
            json!({"outcome":"error","error":{"debug":format!("{error:?}"),"display":error.to_string()},"parameter_reads":settings.reads}),
        ),
    }
}
