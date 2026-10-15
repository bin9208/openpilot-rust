//! Actual native CPU models and causal sequence, including full template state.
use openpilot_onnx_runtime::Provider;
use openpilot_signal_observer::{
    neural,
    neural_sequence::{Error, Sequence},
    pixels::Rgb,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    env, fs,
    io::{self, Read},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Frame {
    path: PathBuf,
    width: u32,
    height: u32,
    timestamp: f64,
}
fn main() -> Result<(), Error> {
    openpilot_opencv_runtime::initialize(1)?;
    let mut args = env::args().skip(1);
    let provider =
        Provider::verified(&PathBuf::from(args.next().ok_or("provider root required")?))?;
    let models = PathBuf::from(args.next().ok_or("model root required")?);
    let mut network = neural::cpu(&provider, &models)?;
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Vec<Frame>> = serde_json::from_str(&text)?;
    let mut output = Vec::new();
    for frames in cases {
        let mut sequence = Sequence::default();
        let mut result = Vec::new();
        for frame in frames {
            let rgb = Rgb {
                width: frame.width,
                height: frame.height,
                data: fs::read(frame.path)?,
            };
            let value = sequence.process(&rgb, frame.timestamp, &mut network)?;
            result.push(json!({"result":value,"state":sequence.snapshot()}));
        }
        output.push(result);
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
