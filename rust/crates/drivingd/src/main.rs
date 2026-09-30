mod daemon;
mod process;

use openpilot_driving_modeld::Error;
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use std::{
    env,
    path::PathBuf,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub struct Options {
    catalog: PathBuf,
    frames: Option<u64>,
}

fn options() -> Result<Option<Options>, Error> {
    let mut args = env::args_os().skip(1);
    let mut catalog = None;
    let mut frames = None;
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!("openpilot-driving-modeld --trusted-catalog PATH [--frames N]\n\nRuns the internal driving VisionIPC/native-model/cereal loop. PATH must contain\ntrusted immutable executable artifacts. Omit --frames to run continuously.\nUses original OPENPILOT_PREFIX, PARAMS_ROOT and SEND_RAW_PRED conventions.");
            return Ok(None);
        } else if arg == "--trusted-catalog" && catalog.is_none() {
            catalog = Some(PathBuf::from(
                args.next().ok_or(Error::Contract("missing catalog path"))?,
            ));
        } else if arg == "--frames" && frames.is_none() {
            let count = args
                .next()
                .and_then(|value| value.to_str().and_then(|value| value.parse::<u64>().ok()))
                .filter(|count| *count > 0)
                .ok_or(Error::Contract("invalid frame count"))?;
            frames = Some(count);
        } else {
            return Err(Error::Contract("unknown or duplicate option; see --help"));
        }
    }
    Ok(Some(Options {
        catalog: catalog.ok_or(Error::Contract("--trusted-catalog is required"))?,
        frames,
    }))
}

fn main() -> ExitCode {
    match options().and_then(|options| options.map_or(Ok(()), run)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("modeld: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: Options) -> Result<(), Error> {
    let mut logger = Factory::for_runtime()?.logger();
    logger.emit(
        log_site!(),
        Record::text(Level::Warning, "modeld init".into()),
    )?;
    process::configure()?;
    let stop = Arc::new(AtomicBool::new(false));
    let interrupted = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&interrupted))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let result = daemon::run(options, &mut logger, &stop);
    if interrupted.load(Ordering::Relaxed) {
        logger.emit(
            log_site!(),
            Record::text(Level::Warning, "got SIGINT".into()),
        )?;
    }
    logger.close();
    result
}
