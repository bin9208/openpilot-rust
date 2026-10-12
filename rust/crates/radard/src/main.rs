use openpilot_radard::{runtime, Error};
use std::{env, process::ExitCode};
fn run() -> Result<(), Error> {
    let mut args = env::args().skip(1);
    let mut frames = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" => {
                println!("openpilot-radard [--frames N]\nNative model-polled physical dPath RadarD. --frames bounds host validation.");
                return Ok(());
            }
            "--frames" => {
                let value = args
                    .next()
                    .ok_or(Error::Contract("missing frame count"))?
                    .parse::<u64>()
                    .map_err(|_| Error::Contract("invalid frame count"))?;
                if value == 0 || frames.is_some() {
                    return Err(Error::Contract("positive unique frame count required"));
                }
                frames = Some(value);
            }
            _ => return Err(Error::Contract("unknown radard argument")),
        }
    }
    runtime::run(frames)
}
fn main() -> ExitCode {
    match openpilot_managed_entry::runtime(
        "openpilot.selfdrive.carrot.radar.radard_dpath",
        "radard",
        || match run() {
            Ok(()) => Ok(()),
            Err(Error::Signal(signal))
                if signal == signal_hook::consts::SIGINT
                    && std::env::var_os("MANAGER_DAEMON").is_some() =>
            {
                Err(openpilot_managed_entry::StepError::Interrupted)
            }
            Err(Error::Signal(signal)) => {
                if let Err(error) = signal_hook::low_level::emulate_default_handler(signal) {
                    eprintln!("radard signal: {error}");
                }
                Err(openpilot_managed_entry::StepError::raised(Error::Signal(
                    signal,
                )))
            }
            Err(error) => Err(openpilot_managed_entry::StepError::raised(error)),
        },
    ) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("radard: {error}");
            ExitCode::FAILURE
        }
    }
}
