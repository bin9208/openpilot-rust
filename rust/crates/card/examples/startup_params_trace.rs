use openpilot_card::{identification::Identification, startup};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::Path,
    sync::Arc,
};

#[derive(Deserialize)]
struct Case {
    params: Vec<u8>,
    identification: Identification,
    settings: BTreeMap<String, Vec<u8>>,
    controller: bool,
    user_key: Option<String>,
}

const KEYS: [&str; 10] = [
    "CarName",
    "FingerPrints",
    "FirmwareQueryDone",
    "SecOCKey",
    "CarParamsPrevRoute",
    "CarParams",
    "CarParamsCache",
    "CarParamsPersistent",
    "DisengageOnAccelerator",
    "OpenpilotEnabledToggle",
];

fn trace(case: Case, settings: Arc<Params>) -> Result<Value, Box<dyn std::error::Error>> {
    for (key, value) in case.settings {
        settings.put(&key, &value)?;
    }
    startup::save_identification(&settings, &case.identification)?;
    let reader = capnp::serialize::read_message(
        std::io::Cursor::new(case.params),
        capnp::message::ReaderOptions::new(),
    )?;
    let mut params = capnp::message::Builder::new_default();
    params.set_root(reader.get_root::<car_params::Reader>()?)?;
    let mut warnings = Vec::new();
    let mut warning_states = Vec::new();
    let result = startup::prepare_logged(
        startup::Preparation {
            settings: &settings,
            identification: &case.identification,
            message: params,
            has_controller: case.controller,
            user_key: case.user_key.as_deref(),
        },
        |message| {
            warnings.push(message.to_owned());
            warning_states.push(
                ["CarParamsPrevRoute", "CarParams"]
                    .into_iter()
                    .map(|key| Ok((key, settings.get(key)?)))
                    .collect::<Result<BTreeMap<_, _>, openpilot_params::Error>>(),
            );
        },
    );
    let output = match result {
        Ok(result) => {
            let mut writes = openpilot_card::async_params::AsyncParams::new(Arc::clone(&settings));
            writes.put("CarParamsCache", &result.bytes)?;
            writes.put("CarParamsPersistent", &result.bytes)?;
            writes.finish();
            if let Some(failure) = writes.failures().first() {
                return Err(std::io::Error::other(failure.error.to_string()).into());
            }
            json!({"params": result.bytes, "key": result.secoc_key, "controller": result.controller_available, "error": false})
        }
        Err(startup::Error::SecocHex) => json!({"error": true}),
        Err(error) => return Err(error.into()),
    };
    let values = KEYS
        .into_iter()
        .map(|key| Ok((key, settings.get(key)?)))
        .collect::<Result<BTreeMap<_, _>, openpilot_params::Error>>()?;
    let warning_states = warning_states.into_iter().collect::<Result<Vec<_>, _>>()?;
    Ok(
        json!({"output": output, "settings": values,"warnings":warnings,"warning_states":warning_states}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let output = std::env::args().nth(1).ok_or("missing output path")?;
    let root = Path::new(&output)
        .parent()
        .ok_or("missing output directory")?
        .join("params");
    let mut results = Vec::new();
    for (index, case) in cases.into_iter().enumerate() {
        results.push(trace(
            case,
            Arc::new(Params::open(&root, &format!("case{index}"))?),
        )?);
    }
    std::fs::write(output, serde_json::to_vec(&results)?)?;
    Ok(())
}
