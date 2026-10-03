use openpilot_bluetooth::Config;
use std::io::{self, BufRead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let result = match Config::parse(&line?) {
            Ok(config) => serde_json::to_string(&config)?,
            Err(error) => serde_json::to_string(&serde_json::json!({"error": error.to_string()}))?,
        };
        println!("{result}");
    }
    Ok(())
}
