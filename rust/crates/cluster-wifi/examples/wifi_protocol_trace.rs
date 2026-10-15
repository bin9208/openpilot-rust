#[path = "../tests/support/trace.rs"]
mod trace;
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<serde_json::Value> = serde_json::from_str(&input)?;
    let output: Vec<_> = cases.iter().map(trace::run).collect();
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
