//! Source constructor-primed mode cache and setting-change boundary oracle.
use openpilot_plannerd::signal::transport::{Mode, ModeCache};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Frame {
    now: f64,
    value: i64,
}
#[derive(Deserialize)]
struct Input {
    now: f64,
    initial: i64,
    frames: Vec<Frame>,
}
fn number(mode: Mode) -> i64 {
    match mode {
        Mode::Off => 0,
        Mode::Observe => 1,
        Mode::Prepare => 2,
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Input> = serde_json::from_str(&text)?;
    let mut output = Vec::new();
    for case in cases {
        let mode = Mode::from_parameter(case.initial);
        let mut cache = ModeCache::primed(mode, case.now);
        let mut reads = 1;
        let mut frames = Vec::new();
        for frame in case.frames {
            let mode = cache.read(frame.now, || {
                reads += 1;
                Ok::<_, ()>(frame.value)
            });
            frames.push(json!({"mode":number(mode),"reads":reads}));
        }
        output.push(json!({"constructed":number(mode),"frames":frames}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
