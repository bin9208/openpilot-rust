use openpilot_lagd::{parameters, wire};
use openpilot_logging::producer::Factory;
use openpilot_params::Params;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{self, BufRead},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Request {
    root: PathBuf,
    car: Vec<u8>,
    saved: Option<Vec<u8>>,
    previous: Option<Vec<u8>>,
}
fn number(value: f64) -> Value {
    if value.is_nan() {
        json!("nan")
    } else if value.is_infinite() {
        json!(if value > 0. { "inf" } else { "-inf" })
    } else {
        json!(value)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let row: Request = serde_json::from_str(&line?)?;
        let params = Params::open(&row.root, "test")?;
        if let Some(saved) = row.saved {
            params.put("LiveDelay", &saved)?;
        }
        if let Some(previous) = row.previous {
            params.put("CarParamsPrevRoute", &previous)?;
        }
        let seed = parameters::retrieve(
            &params,
            &wire::car(&row.car)?,
            &mut Factory::for_runtime()?.logger(),
        )?;
        let seed = seed.map(|(value, blocks)| json!([number(value), blocks]));
        println!(
            "{}",
            json!({"seed":seed,"remaining":params.get("LiveDelay")?.map(|value|value.len())})
        );
    }
    Ok(())
}
