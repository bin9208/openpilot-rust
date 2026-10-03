use std::io::{self, BufRead};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let line = line?;
        let result = match openpilot_qcomgpsd::nmea::parse(&line) {
            Ok(Some(message)) => serde_json::json!({"repr":message.to_string()}),
            Ok(None) => serde_json::json!({"ignored":true}),
            Err(error) => serde_json::json!({"error":error.to_string()}),
        };
        println!("{result}");
    }
    Ok(())
}
