//! Hosted-only source oracle entrypoint for the real streaming frame parser.
use openpilot_qcomgpsd::framing::Frames;
use serde::Deserialize;
use std::io::{self, BufRead};
#[derive(Deserialize)]
struct Input {
    chunks: Vec<Vec<u8>>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let mut frames = Frames::default();
        let mut output = Vec::new();
        for chunk in input.chunks {
            frames.extend(&chunk);
            loop {
                match frames.next_frame() {
                    Ok(Some((opcode, payload))) => {
                        output.push(serde_json::json!({"opcode": opcode, "payload": payload}))
                    }
                    Ok(None) => break,
                    Err(error) => output.push(serde_json::json!({"error": error.to_string()})),
                }
            }
        }
        println!("{}", serde_json::to_string(&output)?);
    }
    Ok(())
}
