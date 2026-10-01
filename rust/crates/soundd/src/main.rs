use openpilot_soundd::{
    runtime::{self, Config},
    Error,
};
use std::{
    path::PathBuf,
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};
fn run() -> Result<(), Error> {
    let base = std::env::var_os("BASEDIR")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let mut config = Config {
        assets: base.join("openpilot/selfdrive/assets"),
        library: PathBuf::from("libportaudio.so.2"),
        cycles: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--assets" => {
                config.assets = args
                    .next()
                    .ok_or(Error::Contract("--assets needs path"))?
                    .into()
            }
            "--portaudio-library" => {
                config.library = args
                    .next()
                    .ok_or(Error::Contract("--portaudio-library needs path"))?
                    .into()
            }
            "--cycles" => {
                config.cycles = Some(
                    args.next()
                        .and_then(|v| v.parse::<u64>().ok())
                        .filter(|v| *v > 0)
                        .ok_or(Error::Contract("cycles must be positive"))?,
                )
            }
            "--help" => {
                println!("openpilot-soundd [--assets PATH] [--portaudio-library PATH] [--cycles N]\nNative sound policy and samples with external PortAudio v19 output. Alternate paths support owned host fixtures.");
                return Ok(());
            }
            _ => return Err(Error::Contract("unknown argument")),
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    runtime::run(config, stop)
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("soundd: {error}");
            ExitCode::FAILURE
        }
    }
}
