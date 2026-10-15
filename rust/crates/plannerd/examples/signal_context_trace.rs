use openpilot_plannerd::signal::context::Context;
use serde::Deserialize;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Input {
    now: f64,
    context: Context,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Input> = serde_json::from_str(&text)?;
    println!(
        "{}",
        serde_json::to_string(
            &cases
                .into_iter()
                .map(|i| i.context.geometry(i.now))
                .collect::<Vec<_>>()
        )?
    );
    Ok(())
}
