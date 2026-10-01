use openpilot_qcomgpsd::{decode::Log, framing};
use serde::Deserialize;
use std::io::{self, BufRead};
#[derive(Deserialize)]
struct Input {
    opcode: u8,
    payload: Vec<u8>,
    mono: u64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let output = if input.opcode != framing::DIAG_LOG {
            Ok(None)
        } else {
            Log::parse(&input.payload).and_then(|log| log.publication(input.mono))
        };
        let result = match output {
            Ok(Some(message)) => {
                serde_json::json!({"topic":message.topic,"data":message.bytes,"has_fix":message.has_fix})
            }
            Ok(None) => serde_json::json!({"ignored":true}),
            Err(error) => serde_json::json!({"error":error.to_string()}),
        };
        println!("{result}");
    }
    Ok(())
}
