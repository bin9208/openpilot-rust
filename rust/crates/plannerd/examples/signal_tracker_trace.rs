use openpilot_plannerd::signal::tracker::{Frame, Tracker};
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Vec<Frame>> = serde_json::from_str(&text)?;
    let values: Vec<Vec<_>> = cases
        .iter()
        .map(|frames| {
            let mut tracker = Tracker::default();
            frames
                .iter()
                .map(|f| {
                    tracker.update(f).map(
                        |output| serde_json::json!({"output":output,"state":tracker.snapshot()}),
                    )
                })
                .collect::<Result<_, _>>()
        })
        .collect::<Result<_, _>>()?;
    println!("{}", serde_json::to_string(&values)?);
    Ok(())
}
