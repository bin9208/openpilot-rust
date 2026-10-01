use openpilot_paramsd::{runtime, Error};
use std::path::PathBuf;
fn run() -> Result<(), Error> {
    let mut arguments = std::env::args().skip(1);
    let (mut frames, mut memory) = (None, None);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" => {
                println!("openpilot-paramsd [--frames N] [--memory-root PATH]\nNative vehicle parameter learner. Bounded frames and memory-root are host validation options.");
                return Ok(());
            }
            "--frames" => {
                let value = arguments
                    .next()
                    .ok_or(Error::Contract("missing frames"))?
                    .parse::<u64>()
                    .map_err(|_| Error::Contract("frame count"))?;
                if value == 0 || frames.is_some() {
                    return Err(Error::Contract("positive unique frame count required"));
                }
                frames = Some(value);
            }
            "--memory-root" => {
                if memory.is_some()
                    || !std::env::var("OPENPILOT_PREFIX")
                        .unwrap_or_default()
                        .starts_with("rust-probe-")
                {
                    return Err(Error::Contract(
                        "memory override requires isolated rust-probe- prefix",
                    ));
                }
                memory = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or(Error::Contract("missing memory root"))?,
                ));
            }
            _ => return Err(Error::Contract("unknown argument")),
        }
    }
    runtime::run(frames, memory)
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("paramsd: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
