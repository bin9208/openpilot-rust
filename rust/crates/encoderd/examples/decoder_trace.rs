use serde::Deserialize;
use std::path::PathBuf;
#[derive(Deserialize)]
struct Step {
    packet: PathBuf,
    #[serde(default)]
    reset: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, path] = args.as_slice() else {
        return Err("decoder_trace CASE".into());
    };
    let steps: Vec<Step> = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut decoder = openpilot_encoderd::native::decoder::SoftwareDecoder::new()?;
    let mut frames = Vec::new();
    for step in steps {
        if step.reset {
            decoder.reset()?;
        }
        let frame = decoder.decode(&std::fs::read(step.packet)?)?;
        frames.push(frame.map(|frame|serde_json::json!({"width":frame.width,"height":frame.height,"strides":frame.strides,"planes":frame.planes})));
    }
    println!("{}", serde_json::to_string(&frames)?);
    Ok(())
}
