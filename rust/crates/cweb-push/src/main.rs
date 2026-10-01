use openpilot_cweb_push::{
    cli::Options, helpers, native::Native, Error, Platform, Reporter, StatusKind,
};
use std::{
    io::Write,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn sleep(seconds: f64, stop: &AtomicBool) -> Result<(), Error> {
    let duration =
        Duration::try_from_secs_f64(seconds).map_err(|error| Error::Contract(error.to_string()))?;
    let deadline = Instant::now()
        .checked_add(duration)
        .ok_or_else(|| Error::Contract("sleep duration overflow".into()))?;
    while !stop.load(Ordering::Relaxed) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    }
    Ok(())
}
fn run() -> Result<(), Error> {
    let Some(options) = Options::parse()? else {
        return Ok(());
    };
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut io = Native::new(options.fixture_ip, Arc::clone(&stop))?;
    let mut reporter = Reporter::new(options.config, &mut io);
    println!(
        "[cweb_push] starting iface={} port={} heartbeat_interval={}s",
        reporter.config.iface, reporter.config.port, reporter.config.heartbeat_interval_s
    );
    std::io::stdout().flush()?;
    if options.once {
        let deadline = io.monotonic() + helpers::maximum(reporter.config.debounce_s + 2., 3.);
        while io.monotonic() < deadline && !stop.load(Ordering::Relaxed) {
            if reporter.poll_once(&mut io)? {
                return Ok(());
            }
            sleep(helpers::minimum(options.interval, 0.5), &stop)?;
        }
        if !stop.load(Ordering::Relaxed) {
            io.emit(reporter.status(StatusKind::OnceNoReport, &io))?;
        }
    } else {
        while !stop.load(Ordering::Relaxed) {
            reporter.poll_once(&mut io)?;
            sleep(helpers::maximum(options.interval, 1.), &stop)?;
        }
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) | Err(Error::Stopped) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cweb_push: {error}");
            ExitCode::FAILURE
        }
    }
}
