use openpilot_cluster::simulator::gamepad::{self, DualSenseState, GamepadSample};
use serde::Deserialize;
use serde_json::json;
use std::io::Read;
#[derive(Deserialize)]
struct Frame {
    now: f64,
    sample: GamepadSample,
}
#[derive(Deserialize)]
struct Request {
    start: f64,
    frames: Vec<Frame>,
    axes: Vec<f64>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    let mut state = DualSenseState::new(request.start);
    let frames: Vec<_> = request
        .frames
        .iter()
        .map(|frame| {
            let command = state.read_sample(&frame.sample, frame.now);
            json!({"command":command,"status":state.status_text()})
        })
        .collect();
    let axes: Vec<_> = request
        .axes
        .iter()
        .map(|&value| {
            [
                gamepad::normalize_signed_axis(value),
                gamepad::normalize_trigger_axis(value),
                gamepad::normalize_stick(value),
                gamepad::normalize_view_axis(value),
            ]
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string(&json!({"frames":frames,"axes":axes}))?
    );
    Ok(())
}
