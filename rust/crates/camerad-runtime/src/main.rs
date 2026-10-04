#![forbid(unsafe_code)]

use openpilot_camerad_runtime::{close_logging, initialize_logging, run};
use openpilot_logging::{log_site, native::Logger, record::Level};
use std::{
    error::Error,
    sync::{atomic::AtomicBool, Arc},
};

fn start() -> Result<(), Box<dyn Error>> {
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        rustix::process::Signal::POWER.as_raw(),
    ] {
        signal_hook::flag::register(signal, Arc::clone(&stop))?;
    }
    let mut cores = rustix::thread::CpuSet::new();
    cores.set(6);
    if let Err(error) = rustix::thread::sched_setaffinity(None, &cores) {
        let offroad = openpilot_params::Params::for_runtime()?
            .get_bool("IsOffroad")
            .unwrap_or(false);
        if !offroad {
            return Err(std::io::Error::from(error).into());
        }
    }
    run(&stop)?;
    Ok(())
}

fn main() {
    let device = if std::path::Path::new("/TICI").is_file() {
        "tici"
    } else {
        "pc"
    };
    let result = initialize_logging(device)
        .map_err(|error| Box::new(error) as Box<dyn Error>)
        .and_then(|()| start());
    if let Err(error) = &result {
        if let Ok(logger) = Logger::for_runtime("unknown", device) {
            let _ = logger.emit(
                log_site!(),
                Level::Error,
                format!("camerad failed: {error}"),
            );
        }
        eprintln!("camerad failed: {error}");
    }
    close_logging();
    if result.is_err() {
        std::process::exit(1);
    }
}
