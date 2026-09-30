use std::io::{self, BufRead};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let bytes: Vec<u8> = serde_json::from_str(&line?)?;
        println!(
            "{}",
            serde_json::to_string(&openpilot_registration::utf7::decode_replace(&bytes))?
        );
    }
    Ok(())
}
