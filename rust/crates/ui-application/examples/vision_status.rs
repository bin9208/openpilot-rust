use openpilot_ui_application::vision_status::{DisplayState, Error, Packet};
use serde::Deserialize;
use std::io::Read;
#[derive(Deserialize)]
struct Step {
    payload: Vec<u8>,
    now: i128,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let steps: Vec<Step> = serde_json::from_str(&input)?;
    let mut results = Vec::new();
    for step in steps {
        results.push(match Packet::parse(&step.payload) {
            Ok(packet) => {
                let state = DisplayState::at(Some(&packet), step.now);
                let seconds = state.latency_ms.as_ref().map(|value| value.seconds()).transpose()?;
                serde_json::json!({"accepted":true,"state":state,"seconds":seconds})
            }
            Err(error) => serde_json::json!({"accepted":false,"overflow":matches!(error,Error::LatencyOverflow)}),
        });
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
