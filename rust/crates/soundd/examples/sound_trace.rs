use openpilot_logging::producer::Factory;
use openpilot_soundd::{assets::Assets, Input, Policy};
use serde::Deserialize;
use std::{
    io::{self, Read},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Trace {
    assets: PathBuf,
    language: String,
    engage_volume: f64,
    tizi: bool,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    input: Input,
    frames: usize,
    adjust: f64,
    language: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let trace: Trace = serde_json::from_str(&raw)?;
    let mut logger = Factory::for_runtime()?.logger();
    let assets = Assets {
        root: trace.assets,
        tizi: trace.tizi,
        engage_volume: trace.engage_volume,
    };
    let mut policy = Policy::new(assets.load(&trace.language, &mut logger)?, trace.tizi);
    for step in trace.steps {
        if let Some(language) = step.language {
            policy.playback.sounds = assets.load(&language, &mut logger)?;
            policy.playback.frame = 0;
        }
        let unsupported = policy.step(&step.input);
        let mut output = vec![0.; step.frames];
        policy.playback.render(&mut output)?;
        policy.adjust = step.adjust;
        println!(
            "{}",
            serde_json::json!({"state":policy.snapshot(), "samples":output, "unsupported":unsupported})
        );
    }
    Ok(())
}
