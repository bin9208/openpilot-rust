use openpilot_micd::{runtime, Error};
use std::{
    path::PathBuf,
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};

fn run() -> Result<(), Error> {
    let mut library = PathBuf::from("libportaudio.so.2");
    let mut cycles = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--portaudio" => {
                library = args
                    .next()
                    .ok_or(Error::Contract("--portaudio requires path"))?
                    .into()
            }
            "--cycles" => {
                cycles = Some(
                    args.next()
                        .and_then(|value| value.parse::<u64>().ok())
                        .filter(|value| *value > 0)
                        .ok_or(Error::Contract("--cycles requires positive count"))?,
                )
            }
            "--help" => {
                println!("openpilot-micd [--portaudio PATH] [--cycles N]\nNative mono microphone capture and sound-pressure publication.");
                return Ok(());
            }
            _ => return Err(Error::Contract("unknown argument")),
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    runtime::run(&library, stop, cycles)
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("micd: {error}");
            ExitCode::FAILURE
        }
    }
}
