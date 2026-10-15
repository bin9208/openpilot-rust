use openpilot_signal_observer::backend::{automatic, Device};
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Device> = serde_json::from_str(&text)?;
    println!(
        "{}",
        serde_json::to_string(&cases.into_iter().map(automatic).collect::<Vec<_>>())?
    );
    Ok(())
}
