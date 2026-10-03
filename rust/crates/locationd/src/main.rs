use openpilot_locationd::{runtime, Error};
fn run() -> Result<(), Error> {
    let mut arguments = std::env::args().skip(1);
    let frames = match arguments.next().as_deref() {
        None => None,
        Some("--help") => {
            println!("openpilot-locationd [--frames N]\nContinuous native pose estimator. --frames bounds publications for host validation.");
            return Ok(());
        }
        Some("--frames") => {
            let value = arguments
                .next()
                .ok_or(Error::Contract("missing frame count"))?
                .parse::<u64>()
                .map_err(|_| Error::Contract("frame count"))?;
            if value == 0 {
                return Err(Error::Contract("positive frame count required"));
            }
            Some(value)
        }
        _ => return Err(Error::Contract("unknown argument")),
    };
    if arguments.next().is_some() {
        return Err(Error::Contract("unexpected argument"));
    }
    runtime::run(frames)
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("locationd: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
