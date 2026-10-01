use openpilot_bluetooth::bluez::policy::{self, PromptKind};
use openpilot_logmessaged::JsonValue;
use std::{
    error::Error,
    io::{self, BufRead},
};

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::args().nth(1).as_deref() == Some("--digits") {
        let accepted: Vec<_> = (0..=0x10_ffff)
            .filter(|point| policy::digit(*point))
            .map(|point| (point, policy::decimal(point)))
            .collect();
        println!("{}", serde_json::to_string(&accepted)?);
        return Ok(());
    }
    for line in io::stdin().lock().lines() {
        let request = JsonValue::parse(&line?)?;
        let kind: PromptKind =
            serde_json::from_str(&request.get("kind").ok_or("kind required")?.to_json()?)?;
        let value = request.get("value").ok_or("value required")?;
        let result = match policy::validate(kind, &value) {
            Ok(()) => serde_json::json!({"accepted":true}),
            Err(error) => serde_json::json!({"error":error.to_string()}),
        };
        println!("{result}");
    }
    Ok(())
}
