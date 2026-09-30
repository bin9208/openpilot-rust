use openpilot_amplifier::{Bus, LinuxPlatform, Platform};
use serde::Deserialize;
use serde_json::json;
use std::{io, path::PathBuf};
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Action {
    Read { register: u8 },
    Write { register: u8, value: u8 },
}
#[derive(Deserialize)]
struct Input {
    device: PathBuf,
    actions: Vec<Action>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(std::env::args_os().nth(1).ok_or("expected input path")?);
    let input: Input = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut platform = LinuxPlatform::new(&input.device);
    let mut values = Vec::new();
    let result = match platform.open_bus() {
        Err(error) => Err(error),
        Ok(mut bus) => {
            let result = (|| -> io::Result<()> {
                for action in input.actions {
                    match action {
                        Action::Read { register } => values.push(bus.read_byte(register)?),
                        Action::Write { register, value } => bus.write_byte(register, value)?,
                    }
                }
                Ok(())
            })();
            bus.close().and(result)
        }
    };
    println!(
        "{}",
        json!({"values":values,"errno":result.err().and_then(|error|error.raw_os_error())})
    );
    Ok(())
}
