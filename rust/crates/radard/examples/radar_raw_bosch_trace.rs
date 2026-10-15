//! Offline production Raw adapter gate using exact recorded synthetic Cereal input.
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_radard::replay::raw::Raw;
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf};
#[derive(Deserialize)]
struct Scene {
    topic: String,
    raw: String,
}
#[derive(Deserialize)]
struct Step {
    can: Vec<String>,
    #[serde(rename = "carState")]
    car_state: String,
}
#[derive(Deserialize)]
struct Request {
    car_params: PathBuf,
    dbc: PathBuf,
    numerics: PathBuf,
    settings: BTreeMap<String, i32>,
    flipped: bool,
    scene: Vec<Scene>,
    steps: Vec<Step>,
}
fn decode(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if value.len() % 2 != 0 {
        return Err("odd hex input".into());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02x}").unwrap();
        text
    })
}
fn timestamp(bytes: &[u8]) -> Result<u64, Box<dyn std::error::Error>> {
    let message = serialize::read_message(bytes, ReaderOptions::new())?;
    Ok(message.get_root::<event::Reader>()?.get_log_mono_time())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mut raw = Raw::new(
        &std::fs::read(request.car_params)?,
        &request.dbc,
        &request.numerics,
        request.settings,
        request.flipped,
    )?;
    for scene in request.scene {
        raw.scene(&decode(&scene.raw)?, scene.topic == "livePose")?;
    }
    let mut output = Vec::new();
    for (step, input) in request.steps.into_iter().enumerate() {
        for can in input.can {
            let bytes = decode(&can)?;
            let (publications, errors) = raw.feed(&bytes, timestamp(&bytes)?, true)?;
            if !publications.is_empty() || !errors.is_empty() {
                return Err("CAN alone consumed unmatched state".into());
            }
        }
        let state = decode(&input.car_state)?;
        let (publications, errors) = raw.feed(&state, timestamp(&state)?, false)?;
        output.push(serde_json::json!({"step":step,"errors":errors,"publications":publications.iter().map(|bytes|hex(bytes)).collect::<Vec<_>>()}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
