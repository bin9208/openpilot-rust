use openpilot_plannerd::signal::start::{observation_color, Guard, Input};
use serde_json::json;
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Vec<Input>> = serde_json::from_str(&text)?;
    let output: Vec<Vec<_>> = cases
        .iter()
        .map(|frames| {
            let mut guard = Guard::default();
            frames
                .iter()
                .map(|i| json!({"decision":guard.update(i),"color":observation_color(i),"state":guard.snapshot()}))
                .collect()
        })
        .collect();
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
