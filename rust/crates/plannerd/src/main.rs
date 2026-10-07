use openpilot_plannerd::{runtime, Error};
use std::{env, path::PathBuf, process::ExitCode};

fn run() -> Result<(), Error> {
    let mut artifact = env::var_os("PLANNER_ACADOS")
        .map(PathBuf::from)
        .unwrap_or(env::current_exe()?.with_file_name("plannerd-acados"));
    let mut frames = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" => {
                println!("openpilot-plannerd [--solver DIRECTORY] [--frames N]\nNative serial longitudinal/lateral planner. Requires the pinned acados artifact.\n--frames bounds host validation; normal startup polls modelV2/liveTracks continuously.");
                return Ok(());
            }
            "--solver" => {
                artifact = arguments
                    .next()
                    .ok_or(Error::Contract("missing solver directory"))?
                    .into()
            }
            "--frames" => {
                let limit = arguments
                    .next()
                    .ok_or(Error::Contract("missing frame count"))?
                    .parse::<u64>()
                    .map_err(|_| Error::Contract("invalid frame count"))?;
                if limit == 0 || frames.is_some() {
                    return Err(Error::Contract("positive unique frame count required"));
                }
                frames = Some(limit);
            }
            _ => return Err(Error::Contract("unknown planner argument")),
        }
    }
    runtime::run(frames, &artifact)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("plannerd: {error}");
            ExitCode::FAILURE
        }
    }
}
