use openpilot_hardwared::{
    runtime::{self, Config},
    Error,
};
use std::{
    path::PathBuf,
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};
fn run() -> Result<(), Error> {
    let mut root = PathBuf::from("/");
    let mut cycles = None;
    let mut launcher = std::env::current_exe()?.with_file_name("openpilot-process-child");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                root = args
                    .next()
                    .ok_or(Error::Contract("--root needs path"))?
                    .into()
            }
            "--launcher" => {
                launcher = args
                    .next()
                    .ok_or(Error::Contract("--launcher needs path"))?
                    .into()
            }
            "--cycles" => {
                cycles = Some(
                    args.next()
                        .and_then(|v| v.parse::<u64>().ok())
                        .filter(|v| *v > 0)
                        .ok_or(Error::Contract("cycles must be positive"))?,
                )
            }
            "--help" => {
                println!("openpilot-hardwared [--cycles N] [--root PATH] [--launcher PATH]\nContinuous native hardware daemon; normal Params/msgq/logging. An alternate root is for owned host fixtures. No production selection is changed.");
                return Ok(());
            }
            _ => return Err(Error::Contract("unknown argument")),
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let board = root.join("TICI").is_file();
    let agnos = root.join("AGNOS").is_file();
    runtime::run(
        Config {
            root,
            cycles,
            launcher,
            board,
            agnos,
        },
        stop,
    )
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("hardwared: {error}");
            ExitCode::FAILURE
        }
    }
}
