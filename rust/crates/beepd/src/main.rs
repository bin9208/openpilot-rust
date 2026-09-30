use openpilot_beepd::{runtime, Driver, Error, Shell, Stdout, SystemClock};
use openpilot_params::Params;
use std::{
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};

fn run() -> Result<(), Error> {
    let mut arguments = std::env::args().skip(1);
    let cycles = match arguments.next().as_deref() {
        None => None,
        Some("--help") if arguments.next().is_none() => {
            println!("openpilot-beepd [--cycles N]\nNative selfdriveState GPIO alert daemon. Uses original sudo tee GPIO42 targets.");
            return Ok(());
        }
        Some("--cycles") => Some(
            arguments
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .ok_or(Error::Contract("cycles must be positive"))?,
        ),
        _ => return Err(Error::Contract("unknown argument")),
    };
    if arguments.next().is_some() {
        return Err(Error::Contract("unexpected argument"));
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let driver = Driver::new(
        Arc::new(Params::for_runtime()?),
        Arc::new(Shell),
        Arc::new(SystemClock),
        Arc::new(Stdout),
    );
    runtime::supervise(
        driver,
        stop,
        cycles,
        std::env::var("MANAGER_DAEMON").unwrap_or_else(|_| "beep".into()),
    )
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("beepd: {error}");
            ExitCode::FAILURE
        }
    }
}
