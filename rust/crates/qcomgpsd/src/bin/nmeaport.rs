use openpilot_qcomgpsd::{config::Config, nmea, Error};
use std::{
    process::{Command, ExitCode},
    sync::{atomic::AtomicBool, Arc},
};
fn run() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let mode = args.next();
    if mode.as_deref() == Some("--help") {
        println!("nmeaport [--read DEVICE]\nSeparate GNSS debug helper. --read bypasses modem setup for owned fixtures.");
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    if mode.as_deref() == Some("--read") {
        let path = args.next().ok_or(Error::Protocol("missing NMEA path"))?;
        if args.next().is_some() {
            return Err(Error::Protocol("unexpected argument"));
        }
        return openpilot_qcomgpsd::nmea_io::read(std::path::Path::new(&path), &stop);
    }
    if mode.as_deref() == Some("--fixture") {
        let path = args
            .next()
            .ok_or(Error::Protocol("missing fixture config"))?;
        if args.next().is_some() {
            return Err(Error::Protocol("unexpected argument"));
        }
        return nmea::setup(
            &openpilot_qcomgpsd::daemon::fixture(std::path::Path::new(&path))?,
            &stop,
        );
    }
    if mode.is_some() {
        return Err(Error::Protocol("unknown argument"));
    }
    let status = Command::new("pidof")
        .args(["qcomgpsd", "openpilot-qcomgpsd"])
        .output()?
        .status;
    match status.code() {
        Some(1) => (),
        Some(0) => {
            return Err(Error::Protocol(
                "qcomgpsd is running, please kill openpilot before running this script! (aborted)",
            ))
        }
        value => {
            return Err(Error::Command {
                command: "pidof qcomgpsd".into(),
                status: value.unwrap_or(-1),
            })
        }
    }
    nmea::setup(&Config::default(), &stop)
}
fn main() -> ExitCode {
    match run() {
        Ok(()) | Err(Error::Stopped) => ExitCode::SUCCESS,
        Err(Error::NmeaReboot) => {
            println!("re-run this script when it is back up");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("nmeaport: {error}");
            ExitCode::FAILURE
        }
    }
}
