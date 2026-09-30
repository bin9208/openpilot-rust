use openpilot_deleter::{Cycle, Deleter, Error};
use serde::Deserialize;
use serde_json::json;
use std::{
    ffi::OsStr,
    io::{self, BufRead, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Command {
    Cycle { bytes: String, percent: String },
    Tick,
    Preserved,
}

fn hex(name: &OsStr) -> String {
    name.as_encoded_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn cycle(value: Cycle) -> serde_json::Value {
    json!({"deleted": value.deleted.as_deref().map(hex), "wait_ms": value.wait.as_millis()})
}

fn execute(deleter: &mut Deleter, command: Command) -> Result<serde_json::Value, Error> {
    Ok(match command {
        Command::Cycle { bytes, percent } => cycle(
            deleter.cycle(
                bytes
                    .parse()
                    .map_err(|_| Error::Arguments("bad byte count"))?,
                percent
                    .parse()
                    .map_err(|_| Error::Arguments("bad percent"))?,
            )?,
        ),
        Command::Tick => cycle(deleter.tick()?),
        Command::Preserved => {
            let mut names: Vec<_> = deleter
                .preserved()?
                .iter()
                .map(|value| hex(value))
                .collect();
            names.sort();
            json!({"preserved": names})
        }
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args_os().nth(1).ok_or("missing root")?);
    let mut deleter = Deleter::new(&root);
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let response = match execute(&mut deleter, serde_json::from_str(&line?)?) {
            Ok(value) => value,
            Err(Error::Io(error)) => json!({"error": error.raw_os_error()}),
            Err(error) => return Err(error.into()),
        };
        serde_json::to_writer(&mut output, &response)?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
