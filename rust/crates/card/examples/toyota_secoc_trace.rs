use openpilot_can::Frame;
use openpilot_card::brands::toyota::secoc::{Freshness, Key};
use serde::Deserialize;
use std::io::{self, Read};

#[derive(Deserialize)]
struct Case {
    key: Vec<u8>,
    trip: u16,
    reset: u32,
    message: u64,
    frame: Frame,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let results = cases.into_iter().map(|case| {
        let key = Key::parse(&case.key)?;
        Ok(serde_json::json!({"sync":key.sync_mac(case.trip, case.reset)?,
            "frame":key.authenticate(Freshness { trip: case.trip, reset: case.reset, message: case.message }, case.frame)?}))
    }).collect::<Result<Vec<_>, openpilot_card::brands::toyota::Error>>()?;
    std::fs::write(
        std::env::args().nth(1).ok_or("missing output")?,
        serde_json::to_vec(&results)?,
    )?;
    Ok(())
}
