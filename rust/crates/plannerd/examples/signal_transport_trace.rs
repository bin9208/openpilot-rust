use openpilot_plannerd::signal::transport::{Reader, VehicleContext};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Input {
    initial_enabled: bool,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
struct Frame {
    now: f64,
    requested: bool,
    raw: Option<String>,
    vehicle: VehicleContext,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Input> = serde_json::from_str(&text)?;
    let mut output = Vec::new();
    for case in cases {
        let mut reader = Reader::new(case.initial_enabled);
        let mut values = Vec::new();
        for f in case.frames {
            let (observation, context) = reader.read_with(
                f.now,
                || f.requested,
                || {
                    f.raw
                        .map(String::into_bytes)
                        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
                },
                &f.vehicle,
            );
            values.push(
                json!({"observation":observation,"context":context,"state":reader.snapshot()}),
            );
        }
        output.push(values);
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
