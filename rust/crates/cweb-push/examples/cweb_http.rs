use openpilot_cweb_push::{http::Http, Payload};
use serde::Deserialize;
use std::io::{BufRead, Write};
#[derive(Deserialize)]
struct Input {
    url: String,
    payload: Payload,
    timeout: f64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http = Http::new()?;
    let mut output = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        writeln!(
            output,
            "{}",
            serde_json::to_string(&http.post(&input.url, &input.payload, input.timeout))?
        )?;
        output.flush()?;
    }
    Ok(())
}
