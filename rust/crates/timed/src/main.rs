use openpilot_logging::producer::Factory;
use openpilot_params::Params;
use openpilot_timed::{
    clock::SystemClock,
    runtime::{self, Host},
    timezone::{Internet, Paths, Services},
    Error, Sudo,
};
use std::{
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};
fn run() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let cycles = match args.next().as_deref() {
        None => None,
        Some("--help") if args.next().is_none() => {
            println!("openpilot-timed [--cycles N]\nNative GPS clock and timezone daemon. --cycles bounds host validation.\nUses normal Params, msgq, sudo date/rm/ln and the existing logging path.");
            return Ok(());
        }
        Some("--cycles") => Some(
            args.next()
                .and_then(|arg| arg.parse::<u64>().ok())
                .filter(|count| *count > 0)
                .ok_or(Error::Contract("cycles must be positive"))?,
        ),
        _ => return Err(Error::Contract("unknown argument")),
    };
    if args.next().is_some() {
        return Err(Error::Contract("unexpected argument"));
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let params = Params::for_runtime()?;
    let mut logger = Factory::for_runtime()?.logger();
    let mut commands = Sudo;
    let paths = Paths::default();
    let internet = Internet::default();
    runtime::run(
        &mut Host {
            clock: &SystemClock,
            services: Services {
                params: &params,
                logger: &mut logger,
                commands: &mut commands,
                paths: &paths,
            },
            internet: &internet,
        },
        cycles,
        &stop,
    )
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("timed: {error}");
            ExitCode::FAILURE
        }
    }
}
