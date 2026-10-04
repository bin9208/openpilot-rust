use chrono::Datelike;
use openpilot_ui_application::params::datetime;
use std::io::{self, BufRead};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let input: String = serde_json::from_str(&line?)?;
        let output = datetime::parse(&input).map(|v| {
            serde_json::json!({
                "local":v.local.format("%Y-%m-%dT%H:%M:%S%.6f").to_string(),
                "offset":v.offset_micros,"year":v.local.year()
            })
        });
        println!("{}", serde_json::to_string(&output)?);
    }
    Ok(())
}
