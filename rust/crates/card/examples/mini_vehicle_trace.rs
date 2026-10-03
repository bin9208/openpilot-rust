use openpilot_can::Packet;
use openpilot_card::{
    brands::{body, mock},
    core::{ApplyInput, Error, Message, Vehicle},
};
use openpilot_cereal::car_capnp::{car_control, car_state};
use openpilot_params::Params;
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, Read},
    path::Path,
};

#[derive(Deserialize)]
struct Case {
    brand: String,
    now: u64,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    now: u64,
    packets: Vec<Packet>,
    control: Vec<u8>,
    gps: Vec<Vec<u8>>,
}

fn step(vehicle: &mut impl Vehicle, input: Step) -> Result<serde_json::Value, Error> {
    let state = vehicle.update(&input.packets, input.now)?;
    vehicle.commit_state(state.get_root_as_reader::<car_state::Reader>()?)?;
    let control = capnp::serialize::read_message(
        std::io::Cursor::new(input.control),
        capnp::message::ReaderOptions::new(),
    )?;
    let result = vehicle.apply(ApplyInput {
        control: control.get_root::<car_control::Reader>()?,
        now_ns: input.now,
        model: None,
        radar: None,
    })?;
    Ok(
        json!({"state":capnp::serialize::write_message_to_words(&state),"actuators":capnp::serialize::write_message_to_words(&result.actuators),"can":result.can}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    let dbc = std::env::args().nth(2).ok_or("missing DBC root")?;
    let root = Path::new(&path)
        .parent()
        .ok_or("missing output directory")?
        .join("params");
    let mut output = Vec::new();
    for (index, case) in cases.into_iter().enumerate() {
        let settings = Params::open(&root, &format!("case{index}"))?;
        let params: Message = match case.brand.as_str() {
            "body" => body::parameters("COMMA_BODY", &[], &settings)?,
            "mock" => mock::parameters("MOCK", &[], &settings)?,
            _ => return Err("unsupported fixture brand".into()),
        };
        let bytes = capnp::serialize::write_message_to_words(&params);
        let mut results = Vec::new();
        match case.brand.as_str() {
            "body" => {
                let mut vehicle = body::Body::new(&bytes, Path::new(&dbc), case.now)?;
                for input in case.steps {
                    results.push(step(&mut vehicle, input)?);
                }
            }
            "mock" => {
                let mut vehicle = mock::Mock::new()?;
                for input in case.steps {
                    vehicle.update_gps(input.now as f64 / 1e9, &input.gps)?;
                    results.push(step(&mut vehicle, input)?);
                }
            }
            _ => return Err("unsupported fixture brand".into()),
        }
        output.push(json!({"params":bytes,"steps":results}));
    }
    std::fs::write(path, serde_json::to_vec(&output)?)?;
    Ok(())
}
