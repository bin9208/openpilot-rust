use openpilot_lagd::{estimator::Estimator, message, settings::Settings, wire};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, BufRead};
#[derive(Deserialize)]
struct Input {
    values: Vec<f64>,
    blocks: i32,
    idx: usize,
    valid: bool,
    debug: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let mut estimator = Estimator::new(Settings::default(), 0.2)?;
        estimator.reset(0.4, input.blocks)?;
        estimator.blocks.values = input.values;
        estimator.blocks.idx = input.idx;
        let output = match message::packet(&estimator, input.debug)
            .and_then(|value| wire::encode(&value, 123456789, input.valid))
        {
            Ok(bytes) => json!({"bytes":bytes}),
            Err(error) => json!({"error":error.to_string()}),
        };
        println!("{output}");
    }
    Ok(())
}
