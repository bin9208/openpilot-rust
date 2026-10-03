use openpilot_navd::native::http;
use serde_json::json;
use std::io::{self, BufRead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let url: String = serde_json::from_str(&line?)?;
        let result = match http::get(&url) {
            Ok(response) => match response.json {
                Ok(value) => {
                    json!({"ok": true, "status": response.status, "text": response.text, "json_ok": true, "json": value})
                }
                Err(_) => {
                    json!({"ok": true, "status": response.status, "text": response.text, "json_ok": false})
                }
            },
            Err(error) => json!({"ok": false, "error": error.to_string()}),
        };
        println!("{result}");
    }
    Ok(())
}
