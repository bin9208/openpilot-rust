use openpilot_control_tools::{lateral_maneuvers_runtime, Error};
use std::process::ExitCode;

fn run() -> Result<(), Error> {
    let mut frames = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" => {
                println!(
                    "openpilot-lateral-maneuversd [--frames N]\nOriginal six lateral maneuvers with native IPC and CarParams, polling modelV2.\n--frames bounds host verification; SIGINT/SIGTERM stop the process."
                );
                return Ok(());
            }
            "--frames" if frames.is_none() => {
                frames = Some(
                    args.next()
                        .ok_or(Error::Contract("missing frame count"))?
                        .parse()
                        .map_err(|_| Error::Contract("frame count must be positive"))?,
                )
            }
            _ => return Err(Error::Contract("unknown or duplicate argument; see --help")),
        }
    }
    lateral_maneuvers_runtime::run(frames)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lateral maneuvers: {error}");
            ExitCode::FAILURE
        }
    }
}
