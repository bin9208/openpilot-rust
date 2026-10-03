use openpilot_bluetooth::runtime::{self, Paths, RuntimeError};
use std::process::ExitCode;

fn run() -> Result<(), RuntimeError> {
    let mut paths = Paths::default();
    let mut frames = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--help") => {
                println!("openpilot-bluetoothd [--runtime-root PATH] [--config PATH] [--sysfs PATH] [--devices PATH] [--frames N]\nNative Bluetooth evdev reader using normal cereal IPC and command journals. Path and frame overrides support owned host verification.");
                return Ok(());
            }
            Some("--frames") => {
                frames = Some(
                    args.next()
                        .and_then(|value| value.to_str()?.parse::<u64>().ok())
                        .filter(|value| *value > 0)
                        .ok_or(RuntimeError::Contract("frames must be positive"))?,
                );
            }
            Some("--runtime-root" | "--config" | "--sysfs" | "--devices") => {
                let value = args
                    .next()
                    .ok_or(RuntimeError::Contract("path argument requires a value"))?;
                match argument.to_str() {
                    Some("--runtime-root") => paths.runtime = value.into(),
                    Some("--config") => paths.config = value.into(),
                    Some("--sysfs") => paths.sysfs = value.into(),
                    Some("--devices") => paths.devices = value.into(),
                    _ => return Err(RuntimeError::Contract("unknown path argument")),
                }
            }
            _ => return Err(RuntimeError::Contract("unknown argument")),
        }
    }
    if runtime::run(paths, frames)? {
        signal_hook::low_level::emulate_default_handler(signal_hook::consts::SIGINT)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bluetoothd: {error}");
            ExitCode::FAILURE
        }
    }
}
