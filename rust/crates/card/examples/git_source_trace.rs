use openpilot_card::startup::format_git_source;
use serde::Deserialize;
use std::io::{self, Read};

#[derive(Deserialize)]
struct Input {
    remote: Option<String>,
    branch: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = String::new();
    io::stdin().read_to_string(&mut bytes)?;
    let inputs: Vec<Input> = serde_json::from_str(&bytes)?;
    let results: Vec<_> = inputs
        .iter()
        .map(|input| format_git_source(input.remote.as_deref(), input.branch.as_deref()))
        .collect();
    serde_json::to_writer(io::stdout().lock(), &results)?;
    Ok(())
}
