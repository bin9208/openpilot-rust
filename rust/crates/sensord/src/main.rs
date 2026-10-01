use openpilot_sensord::{
    runtime::{self, Config},
    Error,
};
use std::sync::{atomic::AtomicBool, Arc};
fn run() -> Result<(), Error> {
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                config.root = args
                    .next()
                    .ok_or(Error::Contract("--root requires path"))?
                    .into()
            }
            "--launcher" => {
                config.launcher = args
                    .next()
                    .ok_or(Error::Contract("--launcher requires path"))?
                    .into()
            }
            "--help" => {
                println!("openpilot-sensord [--root PATH] [--launcher PATH]\nContinuous native LSM6DS3 daemon. Alternate root requires an isolated rust-probe IPC namespace and owned hardware fixtures.");
                return Ok(());
            }
            _ => return Err(Error::Contract("unknown argument")),
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    runtime::run(config, stop)
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sensord: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
