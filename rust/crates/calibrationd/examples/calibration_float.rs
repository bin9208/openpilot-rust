use openpilot_calibrationd::parameters::parse_float;
use std::io::{self, Read};

fn main() -> Result<(), io::Error> {
    let mut bytes = Vec::new();
    io::stdin().read_to_end(&mut bytes)?;
    match parse_float(&bytes) {
        Ok(value) => println!("{value}"),
        Err(_) => println!("ERROR"),
    }
    Ok(())
}
