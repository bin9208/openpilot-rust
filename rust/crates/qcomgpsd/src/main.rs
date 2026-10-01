use openpilot_qcomgpsd::{assistance, config::Config, daemon, Error};
use std::{
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};
fn run() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let mode = args.next();
    if mode.as_deref() == Some("--help") {
        println!("openpilot-qcomgpsd [--fixture CONFIG]\nNative diagnostic GNSS daemon; production defaults access modem/GPIO.\nUse --fixture only with owned PTYs and a private root.");
        return Ok(());
    }
    let config = match mode.as_deref() {
        None => Config::default(),
        Some("--fixture") => daemon::fixture(std::path::Path::new(
            &args
                .next()
                .ok_or(Error::Protocol("missing fixture config"))?,
        ))?,
        Some("--assistance-worker") => serde_json::from_str(
            &args
                .next()
                .ok_or(Error::Protocol("missing assistance config"))?,
        )?,
        Some(_) => return Err(Error::Protocol("unknown argument")),
    };
    if args.next().is_some() {
        return Err(Error::Protocol("unexpected argument"));
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    if mode.as_deref() == Some("--assistance-worker") {
        signal_hook::flag::register(signal_hook::consts::SIGUSR1, Arc::clone(&stop))?;
        assistance::run(&config, &stop)
    } else {
        match daemon::run(&config, &stop) {
            Err(Error::Stopped) => Ok(()),
            result => result,
        }
    }
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("qcomgpsd: {error}");
            ExitCode::FAILURE
        }
    }
}
