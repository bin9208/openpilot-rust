use openpilot_plannerd::signal::transport::Mode;
use openpilot_signal_observer::runtime;
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Vec<u8>> = serde_json::from_str(&input)?;
    let output = cases
        .iter()
        .map(
            |raw| match Mode::from_parameter(runtime::parameter_value(raw)) {
                Mode::Off => 0,
                Mode::Observe => 1,
                Mode::Prepare => 2,
            },
        )
        .collect::<Vec<_>>();
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
