use openpilot_ublox::{native::Config, Error};
use std::sync::{atomic::AtomicBool, Arc};
fn run() -> Result<(), Error> {
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => config.root = args.next().ok_or(Error::Malformed("--root path"))?.into(),
            "--launcher" => {
                config.launcher = args
                    .next()
                    .ok_or(Error::Malformed("--launcher path"))?
                    .into()
            }
            "--assist-url" => {
                config.assist_url = args.next().ok_or(Error::Malformed("--assist-url URL"))?
            }
            "--help" => {
                println!("openpilot-pigeond [--root PATH] [--launcher PATH] [--assist-url URL]");
                return Ok(());
            }
            _ => return Err(Error::Malformed("unknown argument")),
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    openpilot_ublox::runtime::receiver(config, stop)
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pigeond: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
