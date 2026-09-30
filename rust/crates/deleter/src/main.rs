use openpilot_deleter::{platform, Deleter, Error};
use std::{
    env,
    num::NonZeroU64,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

fn options() -> Result<Option<Option<NonZeroU64>>, Error> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None => Ok(Some(None)),
        Some("--help") if args.next().is_none() => {
            println!("openpilot-deleter [--cycles N]\n\nContinuously applies the original log-space retention policy.\nUses LOG_ROOT or original HOME/TICI paths; --cycles bounds iterations for host QA.");
            Ok(None)
        }
        Some("--cycles") => {
            let cycles = args
                .next()
                .ok_or(Error::Arguments("missing cycle count"))?
                .parse()
                .map_err(|_| Error::Arguments("cycle count must be positive"))?;
            if args.next().is_some() {
                return Err(Error::Arguments("unexpected argument"));
            }
            Ok(Some(Some(cycles)))
        }
        _ => Err(Error::Arguments("unknown argument; see --help")),
    }
}

fn run(cycles: Option<NonZeroU64>) -> Result<(), Error> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut deleter = Deleter::new(&platform::log_root()?);
    let mut remaining = cycles.map(NonZeroU64::get);
    eprintln!("deleter: ready");
    while !stop.load(Ordering::Relaxed) {
        let step = deleter.tick()?;
        if let Some(count) = &mut remaining {
            *count -= 1;
            if *count == 0 {
                break;
            }
        }
        let until = Instant::now() + step.wait;
        while !stop.load(Ordering::Relaxed) {
            let duration = until.saturating_duration_since(Instant::now());
            if duration.is_zero() {
                break;
            }
            thread::sleep(duration.min(Duration::from_millis(20)));
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match options().and_then(|value| value.map_or(Ok(()), run)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("deleter: {error}");
            ExitCode::FAILURE
        }
    }
}
