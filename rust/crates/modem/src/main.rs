use openpilot_modem::{config::Config, runtime::Modem, Error};
use std::{
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
fn run() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let config = match args.next().as_deref() {
        None => Config::default(),
        Some("--config") => {
            let path = args
                .next()
                .ok_or(Error::Contract("--config requires a JSON path"))?;
            serde_json::from_reader(std::fs::File::open(path)?)?
        }
        Some("--help") => {
            println!("openpilot-modem [--config PATH]\nNative modem/PPP daemon. Config overrides are for offline fixtures; defaults address the device modem.");
            return Ok(());
        }
        _ => return Err(Error::Contract("unknown argument")),
    };
    if args.next().is_some() {
        return Err(Error::Contract("unexpected argument"));
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    let mut modem = Modem::new(config);
    modem.run(&stop)?;
    // The first request ends the runtime phase; restoration keeps its original
    // unbounded command contract. A subsequent signal can cancel that phase too.
    stop.store(false, Ordering::Relaxed);
    match modem.stop_controlled(&stop) {
        Err(Error::Io(error))
            if error.kind() == std::io::ErrorKind::Interrupted && stop.load(Ordering::Relaxed) =>
        {
            Ok(())
        }
        result => result,
    }
}
fn main() -> ExitCode {
    match openpilot_managed_entry::runtime("openpilot.system.hardware.tici.modem", "modem", || {
        run().map_err(openpilot_managed_entry::StepError::raised)
    }) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("modem: {error}");
            ExitCode::FAILURE
        }
    }
}
