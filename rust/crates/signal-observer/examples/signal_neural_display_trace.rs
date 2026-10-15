//! Actual native producer evidence mapping, before control transport validation.
use serde_json::Value;
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Vec<Value>> = serde_json::from_str(&text)?;
    let output = cases
        .iter()
        .map(|tracks| openpilot_signal_observer::transport::neural_tracks(tracks))
        .collect::<Result<Vec<_>, _>>()?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
