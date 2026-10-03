use openpilot_usbgpu::{
    check::{self, Options},
    Error,
};
use std::{
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

fn run() -> Result<(), Error> {
    let mut options = Options::for_runtime()?;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--timeout-seconds" => {
                let seconds = args
                    .next()
                    .ok_or(Error::Contract("missing timeout seconds"))?
                    .parse::<f64>()
                    .map_err(|_| Error::Contract("invalid timeout seconds"))?;
                if !seconds.is_finite() {
                    return Err(Error::Contract("invalid timeout seconds"));
                }
                options.timeout = Duration::try_from_secs_f64(seconds.max(0.))
                    .map_err(|_| Error::Contract("invalid timeout seconds"))?;
            }
            "--allow-link-errors" => options.require_clean_link = false,
            "--help" => {
                println!("openpilot-usbgpu-check [--timeout-seconds 15] [--allow-link-errors]\nReturns one JSON error=null|string diagnostic; timeout applies per probe attempt.");
                return Ok(());
            }
            _ => return Err(Error::Contract("unexpected GPU check argument")),
        }
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&cancelled))?;
    }
    println!(
        "{}",
        serde_json::json!({"error":check::run(&options,&cancelled)?})
    );
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("usbgpu check: {error}");
            ExitCode::FAILURE
        }
    }
}
