use openpilot_logging::communication::snapshot_at;
use openpilot_messaging::{
    frequency,
    state::{Error, Options, State},
};
use serde::Deserialize;
use std::{
    error::Error as StdError,
    io::{self, BufRead, Write},
};

#[derive(Deserialize)]
struct Configuration {
    services: Vec<String>,
    options: Options,
}
#[derive(Deserialize)]
struct Frame {
    configuration: Option<Configuration>,
    time: f64,
    snapshot_time: f64,
    messages: Vec<Vec<u8>>,
    services: Vec<String>,
}

fn main() -> Result<(), Box<dyn StdError>> {
    let mut state = None;
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        if let Some(configuration) = frame.configuration {
            state = Some(State::new(
                &configuration
                    .services
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                configuration.options,
            )?);
        }
        let state = state.as_mut().ok_or("configuration required")?;
        match state.update(frame.time, &frame.messages) {
            Ok(()) | Err(Error::Frequency(frequency::Error::ZeroInterval)) => (),
            Err(error) => return Err(error.into()),
        }
        writeln!(
            output,
            "{}",
            snapshot_at(
                state,
                &frame
                    .services
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                frame.snapshot_time
            )?
            .to_json()?
        )?;
    }
    output.flush()?;
    Ok(())
}
