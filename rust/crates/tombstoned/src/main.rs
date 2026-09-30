use openpilot_crash_reporting::{sdk::NativeSdk, Reporter, RuntimeInputs};
use openpilot_logging::producer::Factory;
use openpilot_tombstoned::{
    apport::Retrace,
    daemon::{Daemon, WallClock},
    options::Options,
    Error, SCAN_INTERVAL,
};
use std::{
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn run(options: Options) -> Result<(), Error> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let sdk = match &options.local_dsn {
        Some(dsn) => NativeSdk::local_capture(dsn)?,
        None => NativeSdk::default(),
    };
    let mut inputs = RuntimeInputs::new(options.base);
    if let Some(device) = options.local_device {
        inputs.pc = false;
        inputs.device_override = Some(device);
    }
    let reporter = Reporter {
        sdk,
        inputs,
        logger: Factory::for_runtime()?.logger(),
    };
    let mut daemon = Daemon::start(
        reporter,
        options.apport,
        options.log_root,
        Retrace::default(),
        WallClock,
    )?;
    eprintln!("tombstoned: ready reporting={}", daemon.should_report());
    let mut remaining = options.cycles.map(std::num::NonZeroU64::get);
    while !stop.load(Ordering::Relaxed) {
        daemon.cycle()?;
        if let Some(cycles) = &mut remaining {
            *cycles -= 1;
            if *cycles == 0 {
                break;
            }
        }
        let until = Instant::now() + SCAN_INTERVAL;
        while !stop.load(Ordering::Relaxed) {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            std::thread::sleep(left.min(Duration::from_millis(20)));
        }
    }
    Ok(())
}
fn main() -> ExitCode {
    match Options::parse(std::env::args_os().skip(1))
        .and_then(|options| options.map_or(Ok(()), run))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tombstoned: {error}");
            ExitCode::FAILURE
        }
    }
}
