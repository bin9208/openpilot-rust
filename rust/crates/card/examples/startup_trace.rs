use openpilot_can::Packet;
use openpilot_card::{
    fingerprint::{catalog, Fingerprint},
    toggle::{Button, MainToggle},
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Deserialize)]
struct Request {
    fingerprints: Vec<Vec<Vec<Packet>>>,
    toggles: Vec<Vec<ToggleStep>>,
}
#[derive(Deserialize)]
struct ToggleStep {
    buttons: Vec<Button>,
    engaged: bool,
    now: f64,
}
#[derive(Serialize)]
struct FingerprintOutput {
    selected: Option<String>,
    observed: Vec<(u8, Vec<(u32, usize)>)>,
    frames: u64,
}
#[derive(Serialize)]
struct ToggleOutput {
    fired: bool,
    pressed_at: Option<f64>,
    triggered: bool,
}
#[derive(Serialize)]
struct Output {
    fingerprints: Vec<FingerprintOutput>,
    toggles: Vec<Vec<ToggleOutput>>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    let legacy = catalog()?;
    let fingerprints = request
        .fingerprints
        .into_iter()
        .map(|batches| {
            let mut state = Fingerprint::new(legacy.clone());
            for batch in batches {
                state.observe(&batch);
                if state.done {
                    break;
                }
            }
            FingerprintOutput {
                selected: state.selected,
                observed: state.observed,
                frames: state.frames,
            }
        })
        .collect();
    let toggles = request
        .toggles
        .into_iter()
        .map(|steps| {
            let mut state = MainToggle::new(8);
            steps
                .into_iter()
                .map(|step| {
                    let fired = state.update(&step.buttons, (step.engaged, step.now));
                    ToggleOutput {
                        fired,
                        pressed_at: state.pressed_at,
                        triggered: state.triggered,
                    }
                })
                .collect()
        })
        .collect();
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(
        path,
        serde_json::to_vec(&Output {
            fingerprints,
            toggles,
        })?,
    )?;
    Ok(())
}
