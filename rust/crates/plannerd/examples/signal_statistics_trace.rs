use openpilot_plannerd::signal::statistics::{median, percentile};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Input {
    values: Vec<f64>,
    percent: f64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Input> = serde_json::from_str(&text)?;
    let values: Vec<_> = cases
        .iter()
        .map(|i| json!({"percentile":percentile(&i.values,i.percent),"median":median(&i.values)}))
        .collect();
    println!("{}", serde_json::to_string(&values)?);
    Ok(())
}
