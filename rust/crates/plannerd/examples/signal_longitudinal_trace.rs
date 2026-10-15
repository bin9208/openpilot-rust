use openpilot_plannerd::signal::longitudinal::{Constraints, Input, Output};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Frame {
    input: Input,
    output: Output,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Vec<Frame>> = serde_json::from_str(&text)?;
    let values: Vec<Vec<_>> = cases
        .into_iter()
        .map(|frames| {
            let mut constraints = Constraints::default();
            frames
                .into_iter()
                .map(|mut f| {
                    constraints.apply(&f.input, &mut f.output);
                    json!({"output":f.output,"state":constraints.snapshot()})
                })
                .collect()
        })
        .collect();
    println!("{}", serde_json::to_string(&values)?);
    Ok(())
}
