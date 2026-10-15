use openpilot_signal_observer::{pixels::Rgb, process::Process};
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Frame {
    path: PathBuf,
    width: u32,
    height: u32,
    timestamp: f64,
    allow_green: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    openpilot_opencv_runtime::initialize(1)?;
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Vec<Frame>> = serde_json::from_str(&text)?;
    let mut output = Vec::new();
    for frames in cases {
        let mut process = Process::default();
        let mut results = Vec::new();
        for f in frames {
            let rgb = Rgb {
                width: f.width,
                height: f.height,
                data: fs::read(f.path)?,
            };
            let tracks = process.process(&rgb, f.timestamp, f.allow_green)?;
            results.push(json!({"tracks":tracks,"state":process.snapshot()}));
        }
        output.push(results);
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
