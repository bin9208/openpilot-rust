use openpilot_signal_observer::transport;
use serde_json::{json, Value};
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let records: Vec<Value> = serde_json::from_str(&text)?;
    let results=records.iter().map(|record|Ok(json!({"start":transport::start(record)?,"prepare":transport::prepare(record)?,"display":transport::display(record)?}))).collect::<Result<Vec<_>,transport::Error>>()?;
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
