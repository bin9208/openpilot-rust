use openpilot_loggerd::{daemon, metadata::Environment, Error};
use std::process::ExitCode;

fn run() -> Result<(), Error> {
    let mut args = std::env::args_os().skip(1);
    match args.next() {
        Some(argument) if argument == "--help" => {
            println!("openpilot-loggerd\n\nContinuous native route logger. Environment and Params follow the original\nloggerd. Production daemon selection is unchanged. Host QA may set LOG_ROOT,\nPARAMS_ROOT and OPENPILOT_PREFIX to isolated directories/namespaces.");
            Ok(())
        }
        Some(_) => Err(Error::Invalid("unexpected argument; see --help")),
        None => daemon::run(Environment::read()?),
    }
}

fn main() -> ExitCode {
    let status = match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("loggerd: fatal: {error}");
            ExitCode::FAILURE
        }
    };
    openpilot_loggerd::diagnostics::close();
    status
}
