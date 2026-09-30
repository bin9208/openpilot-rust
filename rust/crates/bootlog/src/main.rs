use openpilot_bootlog::{capture, Inputs};
use openpilot_loggerd::{diagnostics, metadata::Environment};
use std::{path::PathBuf, process::ExitCode};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut inputs = Inputs::default();
    let mut args = std::env::args_os().skip(1);
    while let Some(argument) = args.next() {
        let value = PathBuf::from(args.next().ok_or("missing fixture path")?);
        match argument.to_str() {
            Some("--pstore") => inputs.pstore = value,
            Some("--launch-log") => inputs.launch_log = value,
            _ => return Err("expected --pstore PATH or --launch-log PATH".into()),
        }
    }
    let environment = Environment::read()?;
    diagnostics::initialize(environment.device_name()?);
    let result = capture(&environment, &inputs);
    diagnostics::close();
    result?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bootlog: {error}");
            ExitCode::FAILURE
        }
    }
}
