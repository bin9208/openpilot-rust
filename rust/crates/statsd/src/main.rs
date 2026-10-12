use openpilot_statsd::daemon::{run, Configuration, SystemClock};
use std::{
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};
fn main() -> ExitCode {
    let result = openpilot_managed_entry::runtime("openpilot.system.statsd", "statsd", || {
        run_daemon().map_err(openpilot_managed_entry::StepError::raised)
    });
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("statsd: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_daemon() -> Result<(), openpilot_statsd::Error> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    run(&Configuration::for_runtime()?, &mut SystemClock, &stop)
}
