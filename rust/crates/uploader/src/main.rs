use openpilot_uploader::{runtime, Error};
use std::{env, process::ExitCode};
fn run() -> Result<(), Error> {
    let mut args = env::args().skip(1);
    let cycles = match args.next().as_deref() {
        None => None,
        Some("--help") if args.next().is_none() => {
            println!("openpilot-uploader [--cycles N]\n\nNative log uploader. Uses normal Params, deviceState, LOG_ROOT and API_HOST.\n--cycles bounds iterations for host validation.");
            return Ok(());
        }
        Some("--cycles") => Some(
            args.next()
                .and_then(|arg| arg.parse::<u64>().ok())
                .filter(|count| *count > 0)
                .ok_or(Error::Configuration("cycles must be positive"))?,
        ),
        _ => return Err(Error::Configuration("unknown argument; see --help")),
    };
    if args.next().is_some() {
        return Err(Error::Configuration("unexpected argument"));
    }
    runtime::run(cycles)
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("uploader: {error}");
            ExitCode::FAILURE
        }
    }
}
