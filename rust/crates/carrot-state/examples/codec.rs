use openpilot_carrot_state::{encode, Error};
use std::io::{Read, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let service = arguments
        .next()
        .ok_or_else(|| Error::Source("missing service".to_owned()))?;
    let sequence = arguments
        .next()
        .ok_or_else(|| Error::Source("missing sequence".to_owned()))?
        .parse::<u16>()?;
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes)?;
    std::io::stdout().write_all(&encode(&service, &bytes, sequence)?)?;
    Ok(())
}
