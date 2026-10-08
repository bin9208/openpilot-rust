use openpilot_navd::{native, Error};
use std::{env, process::ExitCode};

fn run() -> Result<(), Error> {
    let argument = env::args_os().nth(1);
    let argument = argument
        .as_deref()
        .map(|value| value.to_str().ok_or(Error::Field("destination UTF-8")))
        .transpose()?;
    native::set_destination(argument)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("set_destination: {error}");
            ExitCode::FAILURE
        }
    }
}
