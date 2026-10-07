use openpilot_control_tools::{joystickd_runtime, Error};
use std::{env, num::NonZeroU64, process::ExitCode};

fn run() -> Result<(), Error> {
    let mut frames: Option<NonZeroU64> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" => {
                println!("openpilot-joystickd [--frames N]\nRequires CarParams and native msgq. Runs original 100 Hz joystick control; SIGINT/SIGTERM stop the process.\n--frames bounds host verification publications.");
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
    joystickd_runtime::run(frames)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("joystickd: {error}");
            ExitCode::FAILURE
        }
    }
}
