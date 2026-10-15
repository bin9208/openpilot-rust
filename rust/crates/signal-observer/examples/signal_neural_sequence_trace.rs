//! Source sequence oracle with scripted network results; no inference claim.
use openpilot_signal_observer::{
    neural_sequence::{Color, Error, Network, Proposal, Sequence},
    pixels::Rgb,
};
use serde::Deserialize;
use serde_json::{json, Value};
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
    proposals: Vec<Proposal>,
    color: Color,
}
struct Scripted {
    proposals: Vec<Proposal>,
    color: Color,
    calls: Vec<Value>,
}
impl Network for Scripted {
    fn detect(&mut self, _rgb: &Rgb, rect: [i32; 4]) -> Result<Vec<Proposal>, Error> {
        self.calls.push(json!({"kind":"detect","rect":rect}));
        Ok(self.proposals.clone())
    }
    fn classify(&mut self, _rgb: &Rgb, bounds: [f64; 4]) -> Result<Color, Error> {
        self.calls.push(json!({"kind":"classify","box":bounds}));
        Ok(self.color.clone())
    }
}
fn main() -> Result<(), Error> {
    openpilot_opencv_runtime::initialize(1)?;
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Vec<Frame>> = serde_json::from_str(&input)?;
    let mut outputs = Vec::new();
    for frames in cases {
        let mut sequence = Sequence::default();
        let mut results = Vec::new();
        for frame in frames {
            let rgb = Rgb {
                width: frame.width,
                height: frame.height,
                data: fs::read(frame.path)?,
            };
            let mut network = Scripted {
                proposals: frame.proposals,
                color: frame.color,
                calls: Vec::new(),
            };
            let result = sequence.process(&rgb, frame.timestamp, &mut network)?;
            results
                .push(json!({"result":result,"state":sequence.snapshot(),"calls":network.calls}));
        }
        outputs.push(results);
    }
    println!("{}", serde_json::to_string(&outputs)?);
    Ok(())
}
